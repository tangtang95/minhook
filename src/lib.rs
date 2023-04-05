#![allow(dead_code, non_snake_case, non_camel_case_types)]

use once_cell::sync::OnceCell;
use std::ffi::c_void;
use std::ptr::null_mut;

#[allow(non_camel_case_types)]
#[must_use]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MH_STATUS {
    /// Unknown error. Should not be returned.
    MH_UNKNOWN = -1,
    /// Successful.
    MH_OK = 0,
    /// MinHook is already initialized.
    MH_ERROR_ALREADY_INITIALIZED,
    /// MinHook is not initialized yet, or already uninitialized.
    MH_ERROR_NOT_INITIALIZED,
    /// The hook for the specified target function is already created.
    MH_ERROR_ALREADY_CREATED,
    /// The hook for the specified target function is not created yet.
    MH_ERROR_NOT_CREATED,
    /// The hook for the specified target function is already enabled.
    MH_ERROR_ENABLED,
    /// The hook for the specified target function is not enabled yet, or
    /// already disabled.
    MH_ERROR_DISABLED,
    /// The specified pointer is invalid. It points the address of non-allocated
    /// and/or non-executable region.
    MH_ERROR_NOT_EXECUTABLE,
    /// The specified target function cannot be hooked.
    MH_ERROR_UNSUPPORTED_FUNCTION,
    /// Failed to allocate memory.
    MH_ERROR_MEMORY_ALLOC,
    /// Failed to change the memory protection.
    MH_ERROR_MEMORY_PROTECT,
    /// The specified module is not loaded.
    MH_ERROR_MODULE_NOT_FOUND,
    /// The specified function is not found.
    MH_ERROR_FUNCTION_NOT_FOUND,
}

extern "system" {
    fn MH_Initialize() -> MH_STATUS;
    fn MH_Uninitialize() -> MH_STATUS;
    fn MH_CreateHook(
        pTarget: *mut c_void,
        pDetour: *mut c_void,
        ppOriginal: *mut *mut c_void,
    ) -> MH_STATUS;
    fn MH_EnableHook(pTarget: *mut c_void) -> MH_STATUS;
    fn MH_QueueEnableHook(pTarget: *mut c_void) -> MH_STATUS;
    fn MH_DisableHook(pTarget: *mut c_void) -> MH_STATUS;
    fn MH_QueueDisableHook(pTarget: *mut c_void) -> MH_STATUS;
    fn MH_ApplyQueued() -> MH_STATUS;
}

impl MH_STATUS {
    pub fn ok(self) -> Result<(), MH_STATUS> {
        if self == MH_STATUS::MH_OK {
            Ok(())
        } else {
            Err(self)
        }
    }
}

/// Structure that holds original address, hook function address, and trampoline
/// address for a given hook.
pub struct MhHook {
    addr: *mut c_void,
    hook_impl: *mut c_void,
    trampoline: *mut c_void,
}

static INIT_CELL: OnceCell<()> = OnceCell::new();

impl MhHook {
    /// Create a new hook.
    ///
    /// # Arguments
    ///
    /// * `addr` - Address of the function to hook.
    /// * `hook_impl` - Address of the function to call instead of `addr`.
    ///
    /// # Returns
    ///
    /// A `MhHook` struct that holds the original address, hook function address,
    /// and trampoline address for the given hook.
    ///
    /// # Safety
    ///
    /// `addr` must be a valid address to a function.
    /// `hook_impl` must be a valid address to a function.
    pub unsafe fn new(addr: *mut c_void, hook_impl: *mut c_void) -> Result<Self, MH_STATUS> {
        INIT_CELL.get_or_init(|| {
            let status = unsafe { MH_Initialize() };
            status.ok().expect("Couldn't initialize hooks");
        });

        let mut trampoline = null_mut();
        let status = MH_CreateHook(addr, hook_impl, &mut trampoline);

        status.ok()?;

        Ok(Self {
            addr,
            hook_impl,
            trampoline,
        })
    }

    pub fn trampoline(&self) -> *mut c_void {
        self.trampoline
    }

    unsafe fn queue_enable(&self) {
        let status = MH_QueueEnableHook(self.hook_impl);
    }

    unsafe fn queue_disable(&self) {
        let status = MH_QueueDisableHook(self.hook_impl);
    }
}

/// Wrapper for a queue of hooks to be applied via Minhook.
pub struct MhHooks(Vec<MhHook>);
unsafe impl Send for MhHooks {}
unsafe impl Sync for MhHooks {}

impl MhHooks {
    pub fn new<T: IntoIterator<Item = MhHook>>(hooks: T) -> Result<Self, MH_STATUS> {
        Ok(MhHooks(hooks.into_iter().collect::<Vec<_>>()))
    }

    pub fn apply(&self) {
        unsafe { MhHooks::apply_hooks(&self.0) };
    }

    pub fn unapply(&self) {
        unsafe { MhHooks::unapply_hooks(&self.0) };
        let status = unsafe { MH_Uninitialize() };
    }

    unsafe fn apply_hooks(hooks: &[MhHook]) {
        for hook in hooks {
            let status = MH_QueueEnableHook(hook.addr);
        }
        let status = MH_ApplyQueued();
    }

    unsafe fn unapply_hooks(hooks: &[MhHook]) {
        for hook in hooks {
            let status = MH_QueueDisableHook(hook.addr);
        }
        let status = MH_ApplyQueued();
    }
}

impl Drop for MhHooks {
    fn drop(&mut self) {
        // self.unapply();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::transmute;

    #[test]
    fn test_hooks() {
        unsafe {
            let hooks = MhHooks::new(vec![
                MhHook::new(
                    transmute::<_, *mut c_void>(test_fn as fn() -> i32),
                    transmute::<_, *mut c_void>(test_fn_hook as fn() -> i32),
                )
                .unwrap(),
                MhHook::new(
                    transmute::<_, *mut c_void>(test_fn2 as fn(i32) -> i32),
                    transmute::<_, *mut c_void>(test_fn2_hook as fn(i32) -> i32),
                )
                .unwrap(),
            ])
            .unwrap();

            hooks.apply();

            assert_eq!(test_fn(), 1);
            assert_eq!(test_fn2(1), 2);

            hooks.unapply();

            assert_eq!(test_fn(), 0);
            assert_eq!(test_fn2(1), 1);
        }
    }

    fn test_fn() -> i32 {
        0
    }

    fn test_fn_hook() -> i32 {
        1
    }

    fn test_fn2(x: i32) -> i32 {
        x
    }

    fn test_fn2_hook(x: i32) -> i32 {
        x + 1
    }
}
