// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

use wdk::{nt_success, paged_code, println};
use wdk_sys::{
    call_unsafe_wdf_function_binding,
    ntddk::KeGetCurrentIrql,
    APC_LEVEL,
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    PDRIVER_OBJECT,
    PWDFDEVICE_INIT,
    STATUS_SUCCESS,
    UNICODE_STRING,
    WDFDRIVER,
    WDFOBJECT,
    WDFSTRING,
    WDF_DRIVER_CONFIG,
    WDF_DRIVER_VERSION_AVAILABLE_PARAMS,
    WDF_NO_HANDLE,
    WDF_NO_OBJECT_ATTRIBUTES,
};

use crate::{device, WDF_DRIVER_CONFIG_SIZE, WDF_DRIVER_VERSION_AVAILABLE_PARAMS_SIZE};

extern crate alloc;

use alloc::{slice, string::String};

/// `DriverEntry` initializes the driver and is the first routine called by the
/// system after the driver is loaded. `DriverEntry` specifies the other entry
/// points in the function driver, such as `EvtDevice` and `DriverUnload`.
///
/// # Arguments
///
/// * `driver` - represents the instance of the function driver that is loaded
///   into memory. `DriverEntry` must initialize members of `DriverObject`
///   before it returns to the caller. `DriverObject` is allocated by the system
///   before the driver is loaded, and it is released by the system after the
///   system unloads the function driver from memory.
/// * `registry_path` - represents the driver specific path in the Registry. The
///   function driver can use the path to store driver related data between
///   reboots. The path does not store hardware instance specific data.
///
/// # Return value:
///
/// * `STATUS_SUCCESS` - if successful,
/// * `STATUS_UNSUCCESSFUL` - otherwise.
#[link_section = "INIT"]
#[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let mut driver_config = WDF_DRIVER_CONFIG {
        Size: WDF_DRIVER_CONFIG_SIZE,
        EvtDriverDeviceAdd: Some(echo_evt_device_add),
        ..WDF_DRIVER_CONFIG::default()
    };
    let driver_handle_output = WDF_NO_HANDLE.cast::<WDFDRIVER>();

    let nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDriverCreate,
            driver as PDRIVER_OBJECT,
            registry_path,
            WDF_NO_OBJECT_ATTRIBUTES,
            &mut driver_config,
            driver_handle_output,
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDriverCreate failed {nt_status:#010X}");
        return nt_status;
    }

    echo_print_driver_version();

    nt_status
}

/// `EvtDeviceAdd` is called by the framework in response to `AddDevice`
/// call from the `PnP` manager. We create and initialize a device object to
/// represent a new instance of the device.
///
/// # Arguments:
///
/// * `_driver` - Handle to a framework driver object created in `DriverEntry`
/// * `device_init` - Pointer to a framework-allocated `WDFDEVICE_INIT`
///   structure.
///
/// # Return value:
///
///   * `NTSTATUS`
#[link_section = "PAGE"]
extern "C" fn echo_evt_device_add(_driver: WDFDRIVER, device_init: PWDFDEVICE_INIT) -> NTSTATUS {
    paged_code!();

    println!("Enter  EchoEvtDeviceAdd");

    let device_init =
        // SAFETY: WDF should always be providing a pointer that is properly aligned, dereferencable per https://doc.rust-lang.org/std/ptr/index.html#safety, and initialized. For the lifetime of the resulting reference, the pointed-to memory is never accessed through any other pointer.
        unsafe {
        device_init
            .as_mut()
            .expect("WDF should never provide a null pointer for device_init")
    };
    device::echo_device_create(device_init)
}

/// This routine shows how to retrieve framework version string and
/// also how to find out to which version of framework library the
/// client driver is bound to.
///
/// # Arguments:
///
/// # Return value:
///
///   * `NTSTATUS`
#[link_section = "INIT"]
fn echo_print_driver_version() -> NTSTATUS {
    // 1) Retreive version string and print that in the debugger.
    //
    let mut string: WDFSTRING = core::ptr::null_mut();
    let mut us: UNICODE_STRING = UNICODE_STRING::default();
    let mut nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfStringCreate,
            core::ptr::null_mut(),
            WDF_NO_OBJECT_ATTRIBUTES,
            &mut string
        )
    };
    if !nt_success(nt_status) {
        println!("Error: WdfStringCreate failed {nt_status:#010X}");
        return nt_status;
    }

    let driver = unsafe { (*wdk_sys::WdfDriverGlobals).Driver };
    nt_status = unsafe {
        call_unsafe_wdf_function_binding!(WdfDriverRetrieveVersionString, driver, string)
    };
    if !nt_success(nt_status) {
        // No need to worry about delete the string object because
        // by default it's parented to the driver and it will be
        // deleted when the driverobject is deleted when the DriverEntry
        // returns a failure status.
        //
        println!("Error: WdfDriverRetrieveVersionString failed {nt_status:#010X}");
        return nt_status;
    }

    unsafe {
        call_unsafe_wdf_function_binding!(WdfStringGetUnicodeString, string, &mut us);
    };
    let driver_version = String::from_utf16_lossy(unsafe {
        slice::from_raw_parts(
            us.Buffer,
            us.Length as usize / core::mem::size_of_val(&(*us.Buffer)),
        )
    });
    println!("Echo Sample {driver_version}");

    unsafe {
        call_unsafe_wdf_function_binding!(WdfObjectDelete, string as WDFOBJECT);
    };

    // 2) Find out to which version of framework this driver is bound to.
    //
    let mut ver = WDF_DRIVER_VERSION_AVAILABLE_PARAMS {
        Size: WDF_DRIVER_VERSION_AVAILABLE_PARAMS_SIZE,
        MajorVersion: 1,
        MinorVersion: 0,
    };

    if unsafe { call_unsafe_wdf_function_binding!(WdfDriverIsVersionAvailable, driver, &mut ver) }
        > 0
    {
        println!("Yes, framework version is 1.0");
    } else {
        println!("No, framework version is not 1.0");
    }

    STATUS_SUCCESS
}
