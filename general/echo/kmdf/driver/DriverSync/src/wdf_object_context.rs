// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

use wdk_sys::{PCWDF_OBJECT_CONTEXT_TYPE_INFO, WDF_OBJECT_CONTEXT_TYPE_INFO};

#[repr(transparent)]
pub struct WDFObjectContextTypeInfo(WDF_OBJECT_CONTEXT_TYPE_INFO);
unsafe impl Sync for WDFObjectContextTypeInfo {}

impl WDFObjectContextTypeInfo {
    pub const fn new(inner: WDF_OBJECT_CONTEXT_TYPE_INFO) -> Self {
        Self(inner)
    }

    pub const fn get_unique_type(&self) -> PCWDF_OBJECT_CONTEXT_TYPE_INFO {
        let inner = (self as *const Self).cast::<WDF_OBJECT_CONTEXT_TYPE_INFO>();
        // SAFETY: This dereference is sound since the underlying
        // WDF_OBJECT_CONTEXT_TYPE_INFO is guaranteed to have the same memory
        // layout as WDFObjectContextTypeInfo since WDFObjectContextTypeInfo is
        // declared as repr(transparent)
        unsafe { *inner }.UniqueType
    }
}

macro_rules! wdf_get_context_type_info {
    ($context_type:ident) => {
        paste::paste! {
            [<WDF_ $context_type:snake:upper _TYPE_INFO>].get_unique_type()
        }
    };
}

pub(crate) use wdf_get_context_type_info;

macro_rules! wdf_declare_context_type_with_name {
    ($context_type:ident , $casting_function:ident) => {
        paste::paste! {
            type [<WDFPointerType$context_type>] = *mut $context_type;

            #[link_section = ".data"]
            pub static [<WDF_ $context_type:snake:upper _TYPE_INFO>]: crate::wdf_object_context::WDFObjectContextTypeInfo = crate::wdf_object_context::WDFObjectContextTypeInfo::new(WDF_OBJECT_CONTEXT_TYPE_INFO {
                Size: core::mem::size_of::<WDF_OBJECT_CONTEXT_TYPE_INFO>() as ULONG,
                ContextName: concat!(stringify!($context_type),'\0').as_bytes().as_ptr().cast(),
                ContextSize: core::mem::size_of::<$context_type>(),
                UniqueType: core::ptr::addr_of!([<WDF_ $context_type:snake:upper _TYPE_INFO>]) as *const WDF_OBJECT_CONTEXT_TYPE_INFO,
                EvtDriverGetUniqueContextType: None,
            });

            pub unsafe fn $casting_function(handle: WDFOBJECT) -> [<WDFPointerType$context_type>] {
                unsafe {
                    call_unsafe_wdf_function_binding!(
                        WdfObjectGetTypedContextWorker,
                        handle,
                        crate::wdf_object_context::wdf_get_context_type_info!($context_type),
                    ).cast()
                }
            }
        }
    };
}

pub(crate) use wdf_declare_context_type_with_name;

macro_rules! wdf_declare_context_type {
    ($context_type:ident) => {
        paste::paste! {
            crate::wdf_object_context::wdf_declare_context_type_with_name!($context_type, [<wdf_object_get_ $context_type:snake>]);
        }
    };
}

pub(crate) use wdf_declare_context_type;
