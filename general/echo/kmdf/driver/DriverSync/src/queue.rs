// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0

use core::sync::atomic::Ordering;

use wdk::{nt_success, paged_code, println, wdf};
use wdk_sys::{
    call_unsafe_wdf_function_binding,
    ntddk::{ExAllocatePool2, ExFreePool, KeGetCurrentIrql},
    APC_LEVEL,
    NTSTATUS,
    POOL_FLAG_NON_PAGED,
    SIZE_T,
    STATUS_BUFFER_OVERFLOW,
    STATUS_CANCELLED,
    STATUS_INSUFFICIENT_RESOURCES,
    STATUS_INVALID_DEVICE_REQUEST,
    STATUS_SUCCESS,
    ULONG,
    WDFDEVICE,
    WDFMEMORY,
    WDFOBJECT,
    WDFQUEUE,
    WDFREQUEST,
    WDFTIMER,
    WDF_IO_QUEUE_CONFIG,
    WDF_NO_HANDLE,
    WDF_OBJECT_ATTRIBUTES,
    WDF_TIMER_CONFIG,
    _WDF_EXECUTION_LEVEL,
    _WDF_IO_QUEUE_DISPATCH_TYPE,
    _WDF_SYNCHRONIZATION_SCOPE,
    _WDF_TRI_STATE,
};

use crate::{
    queue_get_context,
    request_get_context,
    wdf_object_context::wdf_get_context_type_info,
    AtomicI32,
    QueueContext,
    RequestContext,
    WDF_QUEUE_CONTEXT_TYPE_INFO,
};

/// Set max write length for testing
const MAX_WRITE_LENGTH: usize = 1024 * 40;

/// Set timer period in ms
const TIMER_PERIOD: u32 = 1000 * 10;

/// This routine will interlock increment a value only if the current value
/// is greater then the floor value.
///
/// The volatile keyword on the Target pointer is absolutely required, otherwise
/// the compiler might rearrange pointer dereferences and that cannot happen.
///
/// # Arguments:
///
/// * `target` - the  value that will be pontetially incrmented
/// * `floor` - the value in which the Target value must be greater then if it
///   is to be incremented
///
/// # Return value:
///
/// The current value of Target.  To detect failure, the return value will be
/// <= Floor + 1.  It is +1 because we cannot increment from the Floor value
/// itself, so Floor+1 cannot be a successful return value.
fn echo_interlocked_increment_floor(target: &AtomicI32, floor: i32) -> i32 {
    let mut current_value = target.load(Ordering::SeqCst);
    loop {
        if current_value <= floor {
            return current_value;
        }

        // currentValue will be the value that used to be Target if the exchange
        // was made or its current value if the exchange was not made.
        //
        match target.compare_exchange(
            current_value,
            current_value + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            // If oldValue == currentValue, then no one updated Target in between
            // the deref at the top and the InterlockecCompareExchange afterward
            // and we have successfully incremented the value and can exit the loop.
            Ok(_) => break,
            Err(v) => current_value = v,
        }
    }

    current_value + 1
}

/// Increment the value only if it is currently > 0.
///
/// # Arguments:
///
/// * `target` - the value to be incremented
///
/// # Return value:
///
/// Upon success, a value > 0.  Upon failure, a value <= 0.
fn echo_interlocked_increment_gtzero(target: &AtomicI32) -> i32 {
    echo_interlocked_increment_floor(target, 0)
}

/// The I/O dispatch callbacks for the frameworks device object
/// are configured in this function.
///
/// A single default I/O Queue is configured for serial request
/// processing, and a driver context memory allocation is created
/// to hold our structure `QUEUE_CONTEXT`.
///
/// This memory may be used by the driver automatically synchronized
/// by the Queue's presentation lock.
///
/// The lifetime of this memory is tied to the lifetime of the I/O
/// Queue object, and we register an optional destructor callback
/// to release any private allocations, and/or resources.
///
/// # Arguments:
///
/// * `device` - Handle to a framework device object.
///
/// # Return value:
///
/// * `NTSTATUS`
#[link_section = "PAGE"]
pub unsafe fn echo_queue_initialize(device: WDFDEVICE) -> NTSTATUS {
    let mut queue = WDF_NO_HANDLE as WDFQUEUE;

    paged_code!();

    // Configure a default queue so that requests that are not
    // configure-fowarded using WdfDeviceConfigureRequestDispatching to goto
    // other queues get dispatched here.
    let mut queue_config = WDF_IO_QUEUE_CONFIG {
        Size: core::mem::size_of::<WDF_IO_QUEUE_CONFIG>() as ULONG,
        PowerManaged: _WDF_TRI_STATE::WdfUseDefault,
        DefaultQueue: u8::from(true),
        DispatchType: _WDF_IO_QUEUE_DISPATCH_TYPE::WdfIoQueueDispatchSequential,
        EvtIoRead: Some(echo_evt_io_read),
        EvtIoWrite: Some(echo_evt_io_write),
        ..WDF_IO_QUEUE_CONFIG::default()
    };

    // Fill in a callback for destroy, and our QUEUE_CONTEXT size
    let mut attributes = WDF_OBJECT_ATTRIBUTES {
        Size: core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>() as ULONG,
        ExecutionLevel: _WDF_EXECUTION_LEVEL::WdfExecutionLevelInheritFromParent,
        SynchronizationScope: _WDF_SYNCHRONIZATION_SCOPE::WdfSynchronizationScopeInheritFromParent,
        ContextTypeInfo: wdf_get_context_type_info!(QueueContext),
        EvtDestroyCallback: Some(echo_evt_io_queue_context_destroy),
        ..WDF_OBJECT_ATTRIBUTES::default()
    };

    // Create queue.
    let nt_status = unsafe {
        call_unsafe_wdf_function_binding!(
            WdfIoQueueCreate,
            device,
            &mut queue_config,
            &mut attributes,
            &mut queue
        )
    };

    if !nt_success(nt_status) {
        println!("WdfIoQueueCreate failed {nt_status:#010X}");
        return nt_status;
    }

    // Get our Driver Context memory from the returned Queue handle
    let queue_context: *mut QueueContext = unsafe { queue_get_context(queue as WDFOBJECT) };
    unsafe {
        (*queue_context).buffer = core::ptr::null_mut();
        (*queue_context).current_request = core::ptr::null_mut();
        (*queue_context).current_status = STATUS_INVALID_DEVICE_REQUEST;
    }

    // Create the SpinLock.
    let mut attributes = WDF_OBJECT_ATTRIBUTES {
        Size: core::mem::size_of::<WDF_OBJECT_ATTRIBUTES>() as ULONG,
        ExecutionLevel: _WDF_EXECUTION_LEVEL::WdfExecutionLevelInheritFromParent,
        SynchronizationScope: _WDF_SYNCHRONIZATION_SCOPE::WdfSynchronizationScopeInheritFromParent,
        ParentObject: queue as WDFOBJECT,
        ..WDF_OBJECT_ATTRIBUTES::default()
    };

    match wdf::SpinLock::create(&mut attributes) {
        Err(status) => {
            println!("SpinLock create failed {nt_status:#010X}");
            return status;
        }
        Ok(spin_lock) => unsafe { (*queue_context).spin_lock = spin_lock },
    };

    // Create the Queue timer
    //
    // By not setting the synchronization scope and using the default at
    // WdfIoQueueCreate, we are explicitly *not* serializing against the queue's
    // lock. Instead, we will do that on our own.
    let mut timer_config = WDF_TIMER_CONFIG {
        Size: core::mem::size_of::<WDF_TIMER_CONFIG>() as ULONG,
        EvtTimerFunc: Some(echo_evt_timer_func),
        Period: TIMER_PERIOD,
        AutomaticSerialization: u8::from(true),
        TolerableDelay: 0,
        ..WDF_TIMER_CONFIG::default()
    };

    match wdf::Timer::create(&mut timer_config, &mut attributes) {
        Err(status) => {
            println!("Timer create failed {nt_status:#010X}");
            return status;
        }
        Ok(wdftimer) => unsafe { (*queue_context).timer = wdftimer },
    };

    STATUS_SUCCESS
}

/// This is called when the Queue that our driver context memory
/// is associated with is destroyed.
///
/// # Arguments:
///
/// * `object` - Queue object to be freed.
///
/// # Return value:
///
/// * `VOID`
extern "C" fn echo_evt_io_queue_context_destroy(object: WDFOBJECT) {
    let queue_context = unsafe { queue_get_context(object) };
    // Release any resources pointed to in the queue context.
    //
    // The body of the queue context will be released after
    // this callback handler returns

    // If Queue context has an I/O buffer, release it
    unsafe {
        if !(*queue_context).buffer.is_null() {
            ExFreePool((*queue_context).buffer);
            (*queue_context).buffer = core::ptr::null_mut();
        }
    }
}

/// Decrements the cancel ownership count for the request.  When the count
/// reaches zero ownership has been acquired.
///
/// # Arguments:
///
/// * `request_context` - the context which holds the count.
///
/// # Return value:
///
/// * TRUE if the caller can complete the request, FALSE otherwise
fn echo_decrement_request_cancel_ownership_count(request_context: *mut RequestContext) -> bool {
    let result = unsafe {
        (*request_context)
            .cancel_completion_ownership_count
            .fetch_sub(1, Ordering::SeqCst)
    };

    result - 1 == 0
}

/// Attempts to increment the request ownership count so that it cannot be
/// completed until the count has been decremented
///
/// # Arguments:
///
/// * `request_context` - the context which holds the count.
///
/// # Return value:
///
/// * TRUE if the count was incremented, FALSE otherwise
fn echo_increment_request_cancel_ownership_count(request_context: *mut RequestContext) -> bool {
    // See comments in echo_interlocked_increment_floor as to why <= 1 is failure
    //
    (unsafe {
        echo_interlocked_increment_gtzero(&(*request_context).cancel_completion_ownership_count)
    }) > 1
}

/// Called when an I/O request is cancelled after the driver has marked
/// the request cancellable. This callback is not automatically synchronized
/// with the I/O callbacks since we have chosen not to use frameworks Device
/// or Queue level locking.
///
/// # Arguments:
///
/// * `request` - Request being cancelled.
///
/// # Return value:
///
/// * `VOID`
extern "C" fn echo_evt_request_cancel(request: WDFREQUEST) {
    let queue = unsafe { call_unsafe_wdf_function_binding!(WdfRequestGetIoQueue, request) };
    let queue_context = unsafe { queue_get_context(queue as WDFOBJECT) };
    let request_context = unsafe { request_get_context(request as WDFOBJECT) };

    println!("echo_evt_request_cancel called on Request {:?}", request);

    // This book keeping is synchronized by the common
    // Queue presentation lock which we are now acquiring
    unsafe { (*queue_context).spin_lock.acquire() };

    let complete_request: bool = echo_decrement_request_cancel_ownership_count(request_context);

    if complete_request {
        unsafe {
            (*queue_context).current_request = core::ptr::null_mut();
        }
    } else {
        unsafe {
            (*queue_context).current_status = STATUS_CANCELLED;
        }
    }

    unsafe { (*queue_context).spin_lock.release() };

    // Complete the request outside of holding any locks
    if complete_request {
        unsafe {
            call_unsafe_wdf_function_binding!(
                WdfRequestCompleteWithInformation,
                request,
                STATUS_CANCELLED,
                0
            );
        }
    }
}

/// Setup the request, intialize its context and mark it as cancelable.
///
/// # Arguments:
///
/// * `request` - Request being set up.
/// * `queue` - Queue associated with the request
///
/// # Return value:
///
/// * `VOID`
fn echo_set_current_request(request: WDFREQUEST, queue: WDFQUEUE) {
    let status: NTSTATUS;
    let request_context = unsafe { request_get_context(request as WDFOBJECT) };
    let queue_context = unsafe { queue_get_context(queue as WDFOBJECT) };

    // Set the ownership count to one.  When a caller wants to claim ownership,
    // they will interlock decrement the count.  When the count reaches zero,
    // ownership has been acquired and the caller may complete the request.
    unsafe {
        (*request_context).cancel_completion_ownership_count = AtomicI32::new(1);
    }

    // Defer the completion to another thread from the timer dpc
    unsafe { (*queue_context).spin_lock.acquire() };
    unsafe {
        (*queue_context).current_request = request;
        (*queue_context).current_status = STATUS_SUCCESS;
    }

    // Set the cancel routine under the lock, otherwise if we set it outside
    // of the lock, the timer could run and attempt to mark the request
    // uncancelable before we can mark it cancelable on this thread. Use
    // WdfRequestMarkCancelableEx here to prevent to deadlock with ourselves
    // (cancel routine tries to acquire the queue object lock).
    unsafe {
        status = call_unsafe_wdf_function_binding!(
            WdfRequestMarkCancelableEx,
            request,
            Some(echo_evt_request_cancel)
        );
        if !nt_success(status) {
            (*queue_context).current_request = core::ptr::null_mut();
        }
    }

    unsafe { (*queue_context).spin_lock.release() };

    unsafe {
        // Complete the request with an error when unable to mark it cancelable.
        if !nt_success(status) {
            call_unsafe_wdf_function_binding!(
                WdfRequestCompleteWithInformation,
                request,
                status,
                0
            );
        }
    }
}

/// This event is called when the framework receives `IRP_MJ_READ` request.
/// It will copy the content from the queue-context buffer to the request
/// buffer. If the driver hasn't received any write request earlier, the read
/// returns zero.
///
/// # Arguments:
///
/// * `queue` - Handle to the framework queue object that is associated with the
///   I/O request.
/// * `request` - Handle to a framework request object.
/// * `length` -  number of bytes to be read. The default property of the queue
///   is to not dispatch zero lenght read & write requests to the driver and
///   complete is with status success. So we will never get a zero length
///   request.
///
/// # Return value:
///
/// * `VOID`
extern "C" fn echo_evt_io_read(queue: WDFQUEUE, request: WDFREQUEST, mut length: usize) {
    let queue_context = unsafe { queue_get_context(queue as WDFOBJECT) };
    let mut memory = WDF_NO_HANDLE as WDFMEMORY;
    let mut nt_status: NTSTATUS;

    println!(
        "echo_evt_io_read called! queue {:?}, request {:?}, length {:?}",
        queue, request, length
    );

    // No data to read
    unsafe {
        if (*queue_context).buffer.is_null() {
            call_unsafe_wdf_function_binding!(
                WdfRequestCompleteWithInformation,
                request,
                STATUS_SUCCESS,
                0,
            );
            return;
        }
    }

    // Read what we have
    unsafe {
        if (*queue_context).length < length {
            length = (*queue_context).length;
        }
    }

    // Get the request memory
    unsafe {
        nt_status =
            call_unsafe_wdf_function_binding!(WdfRequestRetrieveOutputMemory, request, &mut memory);

        if !nt_success(nt_status) {
            println!("echo_evt_io_read Could not get request memory buffer {nt_status:#010X}");
            call_unsafe_wdf_function_binding!(
                WdfRequestCompleteWithInformation,
                request,
                nt_status,
                0
            );
            return;
        }
    }

    // Copy the memory out
    unsafe {
        nt_status = call_unsafe_wdf_function_binding!(
            WdfMemoryCopyFromBuffer,
            memory,
            0,
            (*queue_context).buffer,
            length
        );

        if !nt_success(nt_status) {
            println!("echo_evt_io_read: WdfMemoryCopyFromBuffer failed {nt_status:#010X}");
            call_unsafe_wdf_function_binding!(WdfRequestComplete, request, nt_status);
            return;
        }
    }

    // Set transfer information
    let [()] = unsafe {
        [call_unsafe_wdf_function_binding!(
            WdfRequestSetInformation,
            request,
            length as u64
        )]
    };

    // Mark the request is cancelable.  This must be the last thing we do because
    // the cancel routine can run immediately after we set it.  This means that
    // CurrentRequest and CurrentStatus must be initialized before we mark the
    // request cancelable.
    echo_set_current_request(request, queue);
}

/// This event is invoked when the framework receives `IRP_MJ_WRITE` request.
/// This routine allocates memory buffer, copies the data from the request to
/// it, and stores the buffer pointer in the queue-context with the length
/// variable representing the buffers length. The actual completion of the
/// request is defered to the periodic timer dpc.
///
/// # Arguments:
///
/// * `queue` - Handle to the framework queue object that is associated with the
///   I/O request.
/// * `request` - Handle to a framework request object.
/// * `length` -  number of bytes to be read. The default property of the queue
///   is to not dispatch zero lenght read & write requests to the driver and
///   complete is with status success. So we will never get a zero length
///   request.
///
/// # Return value:
///
/// * `VOID`
extern "C" fn echo_evt_io_write(queue: WDFQUEUE, request: WDFREQUEST, length: usize) {
    let mut memory = WDF_NO_HANDLE as WDFMEMORY;
    let mut status: NTSTATUS;
    let queue_context = unsafe { queue_get_context(queue as WDFOBJECT) };

    println!(
        "echo_evt_io_write called! queue {:?}, request {:?}, length {:?}",
        queue, request, length
    );

    if length > MAX_WRITE_LENGTH {
        println!(
            "echo_evt_io_write Buffer Length to big {:?}, Max is {:?}",
            length, MAX_WRITE_LENGTH
        );
        unsafe {
            call_unsafe_wdf_function_binding!(
                WdfRequestCompleteWithInformation,
                request,
                STATUS_BUFFER_OVERFLOW,
                0
            );
        }
    }

    // Get the memory buffer
    unsafe {
        status =
            call_unsafe_wdf_function_binding!(WdfRequestRetrieveInputMemory, request, &mut memory);
        if !nt_success(status) {
            println!("echo_evt_io_write Could not get request memory buffer {status:#010X}");
            call_unsafe_wdf_function_binding!(WdfRequestComplete, request, status);
            return;
        }
    }

    // Release previous buffer if set
    unsafe {
        if !(*queue_context).buffer.is_null() {
            ExFreePool((*queue_context).buffer);
            (*queue_context).buffer = core::ptr::null_mut();
            (*queue_context).length = 0;
        }

        // FIXME: Memory Tag
        (*queue_context).buffer =
            ExAllocatePool2(POOL_FLAG_NON_PAGED, length as SIZE_T, 's' as u32);
        if (*queue_context).buffer.is_null() {
            println!(
                "echo_evt_io_write Could not allocate {:?} byte buffer",
                length
            );
            call_unsafe_wdf_function_binding!(
                WdfRequestComplete,
                request,
                STATUS_INSUFFICIENT_RESOURCES
            );
            return;
        }
    }

    // Copy the memory in
    unsafe {
        status = call_unsafe_wdf_function_binding!(
            WdfMemoryCopyToBuffer,
            memory,
            0,
            (*queue_context).buffer,
            length
        );

        if !nt_success(status) {
            println!("echo_evt_io_write WdfMemoryCopyToBuffer failed {status:#010X}");
            ExFreePool((*queue_context).buffer);
            (*queue_context).buffer = core::ptr::null_mut();
            (*queue_context).length = 0;
            call_unsafe_wdf_function_binding!(WdfRequestComplete, request, status);
            return;
        }

        (*queue_context).length = length;
    }

    // Set transfer information
    unsafe {
        call_unsafe_wdf_function_binding!(WdfRequestSetInformation, request, length as u64);
    }

    // Mark the request is cancelable.  This must be the last thing we do because
    // the cancel routine can run immediately after we set it.  This means that
    // CurrentRequest and CurrentStatus must be initialized before we mark the
    // request cancelable.
    echo_set_current_request(request, queue);
}

/// This is the `TimerDPC` the driver sets up to complete requests.
/// This function is registered when the WDFTIMER object is created.
///
/// This function does *NOT* automatically synchronize with the I/O Queue
/// callbacks and cancel routine, we must do it ourself in the routine.
///
/// # Arguments:
///
/// * `timer` - Handle to a framework Timer object.
///
/// # Return value:
///
/// * `VOID`
unsafe extern "C" fn echo_evt_timer_func(timer: WDFTIMER) {
    // Default to failure.  status is initialized so that the compiler does not
    // think we are using an uninitialized value when completing the request.
    let mut status;
    let mut cancel = false;
    let complete_request;
    let queue: WDFQUEUE;
    let request: WDFREQUEST;
    let mut request_context: *mut RequestContext = core::ptr::null_mut();
    unsafe {
        queue = call_unsafe_wdf_function_binding!(WdfTimerGetParentObject, timer,) as WDFQUEUE;
    }
    let queue_context = unsafe { queue_get_context(queue as WDFOBJECT) };

    // We must synchronize with the cancel routine which will be taking the
    // request out of the context under this lock.
    unsafe { (*queue_context).spin_lock.acquire() };
    unsafe {
        request = (*queue_context).current_request;
    }
    if !request.is_null() {
        request_context = unsafe { request_get_context(request as WDFOBJECT) };
        if echo_increment_request_cancel_ownership_count(request_context) {
            cancel = true;
        } else {
            // What has happened is that the cancel routine has executed and
            // has already claimed cancel ownership of the request, but has not
            // yet acquired the object lock and cleared the CurrentRequest field
            // in queueContext.  In this case, do nothing and let the cancel
            // routine run to completion and complete the request.
        }
    }

    unsafe { (*queue_context).spin_lock.release() };

    // If we could not claim cancel ownership, we are done.
    if !cancel {
        return;
    }

    // The request handle and requestContext are valid until we release
    // the cancel ownership count we already acquired.
    unsafe {
        status = call_unsafe_wdf_function_binding!(WdfRequestUnmarkCancelable, request,);
        if status != STATUS_CANCELLED {
            println!(
                "CustomTimerDPC successfully cleared cancel routine on request {:?}, status {:?}",
                request, status
            );

            // Since we successfully removed the cancel routine (and we are not
            // currently racing with it), there is no need to use an interlocked
            // decrement to lower the cancel ownership count.

            // 2 is the initial count we set when we initialized
            // CancelCompletionOwnershipCount plus the call to
            // EchoIncrementRequestCancelOwnershipCount()
            (*request_context)
                .cancel_completion_ownership_count
                .fetch_sub(2, Ordering::SeqCst);
            complete_request = true;
        } else {
            complete_request = echo_decrement_request_cancel_ownership_count(request_context);

            if complete_request {
                println!(
                    "CustomTimerDPC Request {:?} is STATUS_CANCELLED, but claimed completion \
                     ownership",
                    request
                );
            } else {
                println!(
                    "CustomTimerDPC Request {:?} is STATUS_CANCELLED, not completing",
                    request
                );
            }
        }
    }

    if complete_request {
        println!(
            "CustomTimerDPC Completing request {:?}, status {:?}",
            request, status
        );

        // Clear the current request out of the queue context and complete
        // the request.
        unsafe { (*queue_context).spin_lock.acquire() };
        unsafe {
            (*queue_context).current_request = core::ptr::null_mut();
            status = (*queue_context).current_status;
        }
        unsafe { (*queue_context).spin_lock.release() };

        unsafe {
            call_unsafe_wdf_function_binding!(WdfRequestComplete, request, status);
        }
    }
}
