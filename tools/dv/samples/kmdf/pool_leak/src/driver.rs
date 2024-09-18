use wdk::{nt_success, paged_code, println};

extern crate alloc;

use wdk_sys::{
    macros,
    ntddk::{ExAllocatePool2, KeGetCurrentIrql},
    APC_LEVEL,
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    PDRIVER_OBJECT,
    POOL_FLAG_NON_PAGED,
    SIZE_T,
    ULONG,
    WDFDEVICE,
    WDFDEVICE_INIT,
    WDFDRIVER,
    WDFOBJECT,
    WDF_DRIVER_CONFIG,
    WDF_NO_HANDLE,
    WDF_NO_OBJECT_ATTRIBUTES,
    WDF_OBJECT_ATTRIBUTES,
    _WDF_EXECUTION_LEVEL,
    _WDF_SYNCHRONIZATION_SCOPE,
};

use crate::{
    wdf_object_context::wdf_get_context_type_info,
    wdf_object_get_device_context,
    GUID_DEVINTERFACE,
    WDF_DEVICE_CONTEXT_TYPE_INFO,
};

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
#[export_name = "DriverEntry"]
extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let mut driver_config = {
        let wdf_driver_config_size: ULONG;

        // clippy::cast_possible_truncation cannot currently check compile-time constants: https://github.com/rust-lang/rust-clippy/issues/9613
        #[allow(clippy::cast_possible_truncation)]
        {
            const WDF_DRIVER_CONFIG_SIZE: usize = core::mem::size_of::<WDF_DRIVER_CONFIG>();

            // Manually assert there is not truncation since clippy doesn't work for
            // compile-time constants
            const { assert!(WDF_DRIVER_CONFIG_SIZE <= ULONG::MAX as usize) }

            wdf_driver_config_size = WDF_DRIVER_CONFIG_SIZE as ULONG;
        }

        WDF_DRIVER_CONFIG {
            Size: wdf_driver_config_size,
            EvtDriverDeviceAdd: Some(evt_driver_device_add),
            EvtDriverUnload: Some(evt_driver_unload),
            ..WDF_DRIVER_CONFIG::default()
        }
    };

    let driver_handle_output = WDF_NO_HANDLE.cast::<WDFDRIVER>();

    let nt_status = unsafe {
        macros::call_unsafe_wdf_function_binding!(
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
extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    paged_code!();

    println!("Enter  EchoEvtDeviceAdd");

    let mut attributes = WDF_OBJECT_ATTRIBUTES {
        Size: core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>() as ULONG,
        ExecutionLevel: _WDF_EXECUTION_LEVEL::WdfExecutionLevelInheritFromParent,
        SynchronizationScope: _WDF_SYNCHRONIZATION_SCOPE::WdfSynchronizationScopeInheritFromParent,
        ContextTypeInfo: wdf_get_context_type_info!(DeviceContext),
        ..WDF_OBJECT_ATTRIBUTES::default()
    };

    let mut device = WDF_NO_HANDLE as WDFDEVICE;
    let mut nt_status = unsafe {
        macros::call_unsafe_wdf_function_binding!(
            WdfDeviceCreate,
            &mut device_init,
            &mut attributes,
            &mut device,
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDeviceCreate failed {nt_status:#010X}");
        return nt_status;
    }

    let device_context = unsafe { wdf_object_get_device_context(device as WDFOBJECT) };
    unsafe {
        let length: usize = 64;
        (*device_context).buffer =
            ExAllocatePool2(POOL_FLAG_NON_PAGED, length as SIZE_T, 's' as u32);
    }

    nt_status = unsafe {
        macros::call_unsafe_wdf_function_binding!(
            WdfDeviceCreateDeviceInterface,
            device,
            &GUID_DEVINTERFACE,
            core::ptr::null_mut(),
        )
    };

    if !nt_success(nt_status) {
        println!("Error: WdfDeviceCreateDeviceInterface failed {nt_status:#010X}");
        return nt_status;
    }

    nt_status
}

extern "C" fn evt_driver_unload(_driver: WDFDRIVER) {
    println!()
}
