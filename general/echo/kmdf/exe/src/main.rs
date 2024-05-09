// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! The below implementation is a bit of a compromise in trying to help new devs
//! find a 1 to 1 mapping to the original C sample app code versus a full proper
//! Rust implementation

//! Idiomatic Rust wrappers for the Windows Driver Kit (WDK) APIs. This crate is
//! built on top of the raw FFI bindings provided by [`wdk-sys`], and provides a
//! safe, idiomatic rust interface to the WDK.
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::multiple_unsafe_ops_per_block)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::unescaped_backticks)]
#![deny(rustdoc::redundant_explicit_links)]

use std::{env, error::Error, ffi::OsString, os::windows::prelude::*, sync::RwLock, thread};

use once_cell::sync::Lazy;
use uuid::{uuid, Uuid};
use windows_sys::Win32::{
    Devices::DeviceAndDriverInstallation,
    Foundation::{
        CloseHandle,
        GetLastError,
        BOOL,
        ERROR_IO_PENDING,
        FALSE,
        HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{
        CreateFileW,
        ReadFile,
        WriteFile,
        FILE_FLAG_OVERLAPPED,
        FILE_GENERIC_READ,
        FILE_GENERIC_WRITE,
        FILE_SHARE_READ,
        FILE_SHARE_WRITE,
        OPEN_EXISTING,
    },
    System::{
        Threading::INFINITE,
        IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED, OVERLAPPED_0},
    },
};

#[derive(Default, Debug)]
struct Globals {
    perform_async_io: bool,
    limited_loops: bool,
    async_io_loops_num: usize,
    device_path: String,
}

static GLOBAL_DATA: Lazy<RwLock<Globals>> = Lazy::new(|| RwLock::new(Globals::default()));
static GUID_DEVINTERFACE_ECHO: Uuid = uuid!("CDC35B6E-0BE4-4936-BF5F-5537380A7C1A");
static READER_TYPE: u32 = 1;
static WRITER_TYPE: u32 = 2;
static NUM_ASYNCH_IO: usize = 100;
static BUFFER_SIZE: usize = 40 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    let argument_vector: Vec<String> = env::args().collect();
    let argument_count = argument_vector.len();

    if argument_count > 1 {
        if argument_vector[1] == "-Async" {
            let mut globals = GLOBAL_DATA.write()?;
            globals.perform_async_io = true;
            if argument_count > 2 {
                globals.async_io_loops_num = argument_vector[2].parse::<usize>()?;
                globals.limited_loops = true;
            } else {
                globals.limited_loops = false;
            }
        } else {
            eprintln!(
                r##"
Usage:
    Echoapp.exe         --- Send single write and read request synchronously
    Echoapp.exe -Async  --- Send reads and writes asynchronously without terminating
    Echoapp.exe -Async <number> --- Send <number> reads and writes asynchronously
Exit the app anytime by pressing Ctrl-C
"##
            );
            return Err("Invalid Args".into());
        }
    }

    get_device_path(&GUID_DEVINTERFACE_ECHO)?;

    let globals = GLOBAL_DATA.read()?;
    println!("DevicePath: {}", globals.device_path);
    let mut path_vec = globals.device_path.encode_utf16().collect::<Vec<_>>();
    let perform_async_io = globals.perform_async_io;
    drop(globals);

    let h_device: HANDLE;
    path_vec.push(0);
    let path = path_vec.as_ptr();

    // SAFETY:
    // Call Win32 API FFI CreateFileW to access driver
    unsafe {
        h_device = CreateFileW(
            path,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            0,
        );
    }

    // SAFETY:
    // Call Win32 API FFI GetLastError() to check for any errors
    unsafe {
        if h_device == INVALID_HANDLE_VALUE {
            return Err(format!("Failed to open device. Error {}", GetLastError()).into());
        }
    }

    println!("Opened device successfully");

    if perform_async_io {
        println!("Starting AsyncIo");

        let h =
            thread::spawn(|| -> Result<(), Box<dyn Error + Send + Sync>> { async_io(READER_TYPE) });

        // Because async_io error requires Send + Sync but this function does not,
        // cannot use ? operator
        #[allow(clippy::question_mark)]
        if let Err(e) = async_io(WRITER_TYPE) {
            return Err(e);
        }

        h.join().unwrap().unwrap();
    } else {
        perform_write_read_test(h_device, 512)?;

        perform_write_read_test(h_device, 30 * 1024)?;
    }

    Ok(())
}

fn create_pattern_buffer(length: u32) -> Vec<u8> {
    let mut buf = Vec::<u8>::with_capacity(usize::try_from(length).unwrap());
    let mut val: u8 = 0;

    for _ in 0..length {
        buf.push(val);
        val = val.wrapping_add(1);
    }

    buf
}

fn verify_pattern_buffer(buf: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut check_value: u8 = 0;
    for val in buf {
        if *val != check_value {
            return Err(format!(
                "Pattern changed.  SB 0x{:02X}, Is 0x{:02X}",
                check_value, *val
            )
            .into());
        }
        check_value = check_value.wrapping_add(1);
    }
    Ok(())
}

fn perform_write_read_test(h_device: HANDLE, test_length: u32) -> Result<(), Box<dyn Error>> {
    let write_buffer = create_pattern_buffer(test_length);
    let mut read_buffer: Vec<u8> = vec![0; usize::try_from(test_length).unwrap()];

    let mut r: BOOL;
    let mut bytes_returned: u32 = 0;

    // SAFETY:
    // Call Win32 API FFI WriteFile to write buffer to the driver
    unsafe {
        r = WriteFile(
            h_device,
            write_buffer.as_ptr().cast(),
            u32::try_from(write_buffer.len()).unwrap(),
            &mut bytes_returned,
            std::ptr::null_mut(),
        );
    }

    // SAFETY:
    // Call Win32 API FFI GetLastError() to check for any errors from WriteFile
    unsafe {
        if r == FALSE {
            return Err(format!(
                "PerformWriteReadTest: WriteFile failed: Error {}",
                GetLastError()
            )
            .into());
        }
    }

    if bytes_returned != test_length {
        return Err(format!(
            "bytes written is not test length! Written {bytes_returned}, SB {test_length}"
        )
        .into());
    }

    println!("{bytes_returned} Pattern Bytes Written successfully");

    bytes_returned = 0;

    // SAFETY:
    // Call Win32 API FFI ReadFile to read data from the driver
    unsafe {
        r = ReadFile(
            h_device,
            read_buffer.as_mut_ptr().cast(),
            test_length,
            &mut bytes_returned,
            std::ptr::null_mut(),
        );
    }

    // SAFETY:
    // Call Win32 API FFI GetLastError() to check for any errors from ReadFile
    unsafe {
        if r == FALSE {
            return Err(format!(
                "PerformWriteReadTest: ReadFile failed: Error {}",
                GetLastError()
            )
            .into());
        }
    }

    // SAFETY:
    // Call set_len on the Vec that contains the buffer used in ReadFile to tell the
    // Vec how many bytes were actually put into the Vec
    unsafe {
        read_buffer.set_len(usize::try_from(bytes_returned).unwrap());
    }

    if bytes_returned != test_length {
        return Err(format!(
            "bytes Read is not test length! Read {bytes_returned}, SB {test_length}"
        )
        .into());
    }

    println!("{bytes_returned} Pattern Bytes Read successfully");

    verify_pattern_buffer(&read_buffer)?;

    println!("Pattern Verified successfully\n");

    Ok(())
}

fn async_io(thread_parameter: u32) -> Result<(), Box<dyn Error + Send + Sync>> {
    match async_io_work(thread_parameter) {
        Err(e) => Err(e.to_string().into()),
        Ok(()) => Ok(()),
    }
}

// In order to keep this function close to the original WDK app, ignoring large
// function warning
#[allow(clippy::too_many_lines)]
fn async_io_work(io_type: u32) -> Result<(), Box<dyn Error>> {
    let globals = GLOBAL_DATA.read()?;

    let h_device: HANDLE;
    let h_completion_port: HANDLE;
    let mut r: BOOL;

    // SAFETY:
    // Call Win32 API FFI CreateFileW to access driver
    unsafe {
        let mut path_vec = globals.device_path.encode_utf16().collect::<Vec<_>>();
        path_vec.push(0);
        let path = path_vec.as_ptr();

        h_device = CreateFileW(
            path,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            0,
        );
    }

    // SAFETY:
    // Call Win32 API FFI GetLastError() to check for any errors from CreateFileW
    unsafe {
        if h_device == INVALID_HANDLE_VALUE {
            return Err(format!(
                "Cannot open {} error {}",
                globals.device_path,
                GetLastError()
            )
            .into());
        }
    }

    // SAFETY:
    // Call Win32 API FFI CreateIoCompletionPort to get handle for completing async
    // requests
    unsafe {
        h_completion_port = CreateIoCompletionPort(h_device, 0, 1, 0);
    }

    // SAFETY:
    // Call Win32 API FFI to check for CreateIoCompletionPort result from
    // GetLastError()
    unsafe {
        // CreateIoCompletionPort returns NULL on failure, not INVALID_HANDLE_VALUE
        if h_completion_port == 0 {
            return Err(format!("Cannot open completion port {}", GetLastError()).into());
        }
    }

    let mut remaining_requests_to_receive = 0;
    let mut max_pending_requests = NUM_ASYNCH_IO;
    let mut remaining_requests_to_send = 0;
    if globals.limited_loops {
        remaining_requests_to_receive = globals.async_io_loops_num;
        if globals.async_io_loops_num > NUM_ASYNCH_IO {
            max_pending_requests = NUM_ASYNCH_IO;
            remaining_requests_to_send = globals.async_io_loops_num - NUM_ASYNCH_IO;
        } else {
            max_pending_requests = globals.async_io_loops_num;
            remaining_requests_to_send = 0;
        }
    }

    let mut ov_list: Vec<OVERLAPPED> = vec![
        OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Anonymous: OVERLAPPED_0 {
                Pointer: std::ptr::null_mut(),
            },
            hEvent: 0,
        };
        max_pending_requests
    ];
    let mut buf: Vec<u8> = vec![0; max_pending_requests * BUFFER_SIZE];

    for i in 0..max_pending_requests {
        // SAFETY:
        // Get the offset into the buffer for sending data at offset for request 'i'
        let buffer_offset = unsafe {
            (buf.as_mut_ptr()
                .offset(isize::try_from(i * BUFFER_SIZE).unwrap()))
            .cast()
        };

        // SAFETY:
        // Get the pointer for the list of Overlapped array for ReadFile at the offset
        // for request 'i'
        let overlap_struct_offset =
            unsafe { ov_list.as_mut_ptr().offset(isize::try_from(i).unwrap()) };

        if io_type == READER_TYPE {
            // SAFETY:
            // Call Win32 API FFI ReadFile to read from driver with an overlap option
            unsafe {
                r = ReadFile(
                    h_device,
                    buffer_offset,
                    u32::try_from(BUFFER_SIZE).unwrap(),
                    std::ptr::null_mut(),
                    overlap_struct_offset,
                );
            }

            // SAFETY:
            // Call Win32 API FFI GetLastError() to check for any errors from ReadFile
            unsafe {
                if r == FALSE {
                    let error = GetLastError();
                    if error != ERROR_IO_PENDING {
                        return Err(format!("{i}th Read failed {error}").into());
                    }
                }
            }
        } else {
            // SAFETY:
            // Call Win32 API FFI WriteFile to write to driver with an overlap option
            unsafe {
                let mut number_of_bytes_written: u32 = 0;

                r = WriteFile(
                    h_device,
                    buffer_offset,
                    u32::try_from(BUFFER_SIZE).unwrap(),
                    &mut number_of_bytes_written,
                    overlap_struct_offset,
                );
            }

            // SAFETY:
            // Call Win32 API FFI GetLastError() to check for any errors from WriteFile
            unsafe {
                if r == FALSE {
                    let error = GetLastError();
                    if error != ERROR_IO_PENDING {
                        return Err(format!("{i}th Write failed {error}").into());
                    }
                }
            }
        }
    }

    loop {
        let mut number_of_bytes_transferred = 0;
        let mut key = 0;
        let mut completed_ov_ptr: *mut OVERLAPPED = std::ptr::null_mut();

        // SAFETY:
        // Call Win32 API FFI GetQueuedCompletionStatus to access the status of the
        // completion request
        unsafe {
            r = GetQueuedCompletionStatus(
                h_completion_port,
                &mut number_of_bytes_transferred,
                &mut key,
                std::ptr::addr_of_mut!(completed_ov_ptr),
                INFINITE,
            );
        }

        // SAFETY:
        // Call Win32 API FFI GetLastError() to check for any errors from
        // GetQueuedCompletionStatus
        unsafe {
            if r == FALSE {
                return Err(format!("GetQueuedCompletionStatus failed {}", GetLastError()).into());
            }
        }

        let i;

        // SAFETY:
        // Perform pointer math to determine which index 'i' to use by determining the
        // offset of 'completed_ov_ptr' from the start of the array given by
        // 'ov_list'
        unsafe {
            i = completed_ov_ptr.offset_from(ov_list.as_ptr());
        }

        if io_type == READER_TYPE {
            println!("Number of bytes read by request number {i} is {number_of_bytes_transferred}",);

            if globals.limited_loops {
                remaining_requests_to_receive -= 1;
                if remaining_requests_to_receive == 0 {
                    break;
                }

                if remaining_requests_to_send == 0 {
                    continue;
                }

                remaining_requests_to_send -= 1;
            }

            let buffer_offset;

            // SAFETY:
            // Get the offset into the buffer for reading data at offset for request 'i'
            unsafe {
                buffer_offset = (buf
                    .as_mut_ptr()
                    .offset(i * isize::try_from(BUFFER_SIZE).unwrap()))
                .cast();
            }

            // SAFETY:
            // Call Win32 API FFI ReadFile to read in data from the driver
            unsafe {
                r = ReadFile(
                    h_device,
                    buffer_offset,
                    u32::try_from(BUFFER_SIZE).unwrap(),
                    std::ptr::null_mut(),
                    completed_ov_ptr,
                );
            }

            // SAFETY:
            // Call Win32 API FFI GetLastError() to check for any errors from ReadFile
            unsafe {
                if r == FALSE {
                    let error = GetLastError();
                    if error != ERROR_IO_PENDING {
                        return Err(format!("{i}th Read failed {error}").into());
                    }
                }
            }
        } else {
            println!(
                "Number of bytes written by request number {i} is {number_of_bytes_transferred}",
            );

            if globals.limited_loops {
                remaining_requests_to_receive -= 1;
                if remaining_requests_to_receive == 0 {
                    break;
                }

                if remaining_requests_to_send == 0 {
                    continue;
                }

                remaining_requests_to_send -= 1;
            }

            let buffer_offset;

            // SAFETY:
            // Get the offset into the buffer for sending data at offset for request 'i'
            unsafe {
                buffer_offset = (buf
                    .as_mut_ptr()
                    .offset(i * isize::try_from(BUFFER_SIZE).unwrap()))
                .cast();
            }

            // SAFETY:
            // Call Win32 API FFI WriteFile to write data to the driver
            unsafe {
                r = WriteFile(
                    h_device,
                    buffer_offset,
                    u32::try_from(BUFFER_SIZE).unwrap(),
                    std::ptr::null_mut(),
                    completed_ov_ptr,
                );
            }

            // SAFETY:
            // Call Win32 API FFI GetLastError() to check for any errors from WriteFile
            unsafe {
                if r == FALSE {
                    let error = GetLastError();
                    if error != ERROR_IO_PENDING {
                        return Err(format!("{i}th write failed {error}").into());
                    }
                }
            }
        }
    }
    drop(globals);

    // SAFETY:
    // Call Win32 API FFI CloseHandle to close completion port handle
    unsafe {
        CloseHandle(h_completion_port);
    }

    // SAFETY:
    // Call Win32 API FFI CloseHandle to close device handle
    unsafe {
        CloseHandle(h_device);
    }

    Ok(())
}

fn get_device_path(interface_guid: &Uuid) -> Result<(), Box<dyn Error>> {
    let mut guid = windows_sys::core::GUID {
        data1: 0,
        data2: 0,
        data3: 0,
        data4: [0, 0, 0, 0, 0, 0, 0, 0],
    };
    let guid_data4: &[u8; 8];
    let mut device_interface_list_length: u32 = 0;
    let mut config_ret;

    (guid.data1, guid.data2, guid.data3, guid_data4) = interface_guid.as_fields();
    guid.data4 = *guid_data4;

    // SAFETY:
    // Call Win32 API FFI CM_Get_Device_Interface_List_SizeW to determine size of
    // space needed for a subsequent request
    unsafe {
        config_ret = DeviceAndDriverInstallation::CM_Get_Device_Interface_List_SizeW(
            &mut device_interface_list_length,
            &guid,
            std::ptr::null(),
            DeviceAndDriverInstallation::CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        );
    }

    if config_ret != DeviceAndDriverInstallation::CR_SUCCESS {
        return Err(
            format!("Error 0x{config_ret:08X} retrieving device interface list size.",).into(),
        );
    }

    if device_interface_list_length <= 1 {
        return Err(
            "Error: No active device interfaces found.  Is the sample driver loaded?".into(),
        );
    }

    let mut buffer: Vec<u16> = vec![0; usize::try_from(device_interface_list_length).unwrap()];
    let buffer_ptr = buffer.as_mut_ptr();

    // SAFETY:
    // Call Win32 API FFI CM_Get_Device_Interface_ListW to get the list of Device
    // Interfaces that match the Interface GUID for the echo driver
    unsafe {
        config_ret = DeviceAndDriverInstallation::CM_Get_Device_Interface_ListW(
            &guid,
            std::ptr::null(),
            buffer_ptr,
            device_interface_list_length,
            DeviceAndDriverInstallation::CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        );
    }

    if config_ret != DeviceAndDriverInstallation::CR_SUCCESS {
        return Err(format!("Error 0x{config_ret:08X} retrieving device interface list.").into());
    }

    let path = OsString::from_wide(buffer.as_slice());

    GLOBAL_DATA.write()?.device_path = path
        .into_string()
        .expect("Unable to convert Device Path to String");

    Ok(())
}
