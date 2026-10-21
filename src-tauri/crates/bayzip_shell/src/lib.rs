#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};
use windows::core::{implement, GUID, HRESULT, PWSTR};
use windows_core::Interface;
use windows::Win32::Foundation::{BOOL, CLASS_E_CLASSNOTAVAILABLE, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER, S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CoTaskMemAlloc, CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl,
};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl,
    IShellItemArray, ECF_DEFAULT, ECF_HASSUBCOMMANDS, ECS_ENABLED, SIGDN_FILESYSPATH,
};

// 44E23C12-5883-4903-9DA9-33D72120C19A
pub const CLSID_BAYZIP_SHELL: GUID = GUID::from_u128(0x44e23c12_5883_4903_9da9_33d72120c19a);

static OBJECT_COUNT: AtomicUsize = AtomicUsize::new(0);

fn to_co_task_mem_pwstr(s: &str) -> windows::core::Result<PWSTR> {
    let utf16: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let size_in_bytes = utf16.len() * std::mem::size_of::<u16>();
    unsafe {
        let ptr = CoTaskMemAlloc(size_in_bytes) as *mut u16;
        if ptr.is_null() {
            return Err(windows::core::Error::from_hresult(E_OUTOFMEMORY));
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
        Ok(PWSTR(ptr))
    }
}

fn get_bayzip_exe_path() -> std::path::PathBuf {
    // 1. Try %LOCALAPPDATA%\Bayzip\bayzip_path.txt
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let path_file = std::path::PathBuf::from(&local_appdata)
            .join("Bayzip")
            .join("bayzip_path.txt");
        if let Ok(content) = std::fs::read_to_string(&path_file) {
            let candidate = std::path::PathBuf::from(content.trim().trim_matches('\"'));
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // 2. Try adjacent to this DLL
    if let Ok(dll_path) = std::env::current_exe() {
        if let Some(parent) = dll_path.parent() {
            let candidate = parent.join("bayzip.exe");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // 3. Try %LOCALAPPDATA%\Bayzip\bayzip.exe
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let candidate = std::path::PathBuf::from(local_appdata)
            .join("Bayzip")
            .join("bayzip.exe");
        if candidate.exists() {
            return candidate;
        }
    }

    // 4. Fallback to just "bayzip.exe" in PATH
    std::path::PathBuf::from("bayzip.exe")
}

fn get_bayzip_icon_path() -> String {
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let ico = std::path::PathBuf::from(local_appdata)
            .join("Bayzip")
            .join("bayzip.ico");
        if ico.exists() {
            return ico.to_string_lossy().to_string();
        }
    }
    let exe = get_bayzip_exe_path();
    format!("{},0", exe.to_string_lossy())
}

fn get_selected_paths(psiitemarray: Option<&IShellItemArray>) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(items) = psiitemarray {
        if let Ok(count) = unsafe { items.GetCount() } {
            for i in 0..count {
                if let Ok(item) = unsafe { items.GetItemAt(i) } {
                    if let Ok(pwstr) = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) } {
                        if let Ok(s) = unsafe { pwstr.to_string() } {
                            paths.push(s);
                        }
                        unsafe { CoTaskMemFree(Some(pwstr.0 as _)) };
                    }
                }
            }
        }
    }
    paths
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    Root,
    CompressGamer,
    CompressBay,
    ExtractHere,
    OpenGui,
}

#[implement(IExplorerCommand)]
pub struct BayzipExplorerCommand {
    action: CommandAction,
}

impl BayzipExplorerCommand {
    pub fn new(action: CommandAction) -> Self {
        OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
        Self { action }
    }
}

impl Drop for BayzipExplorerCommand {
    fn drop(&mut self) {
        OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IExplorerCommand_Impl for BayzipExplorerCommand_Impl {
    fn GetTitle(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<PWSTR> {
        let title = match self.action {
            CommandAction::Root => "Bayzip",
            CommandAction::CompressGamer => "Compress to .bayg (Gamer Preset)",
            CommandAction::CompressBay => "Compress to .bay (Ultra-Fast)",
            CommandAction::ExtractHere => "Extract Here",
            CommandAction::OpenGui => "Open in Bayzip",
        };
        to_co_task_mem_pwstr(title)
    }

    fn GetIcon(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<PWSTR> {
        let icon_path = get_bayzip_icon_path();
        to_co_task_mem_pwstr(&icon_path)
    }

    fn GetToolTip(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<PWSTR> {
        to_co_task_mem_pwstr("Bayzip Archiver")
    }

    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        let u128_id = match self.action {
            CommandAction::Root => 0x44e23c12_5883_4903_9da9_33d72120c19a,
            CommandAction::CompressGamer => 0x44e23c12_5883_4903_9da9_33d72120c19b,
            CommandAction::CompressBay => 0x44e23c12_5883_4903_9da9_33d72120c19c,
            CommandAction::ExtractHere => 0x44e23c12_5883_4903_9da9_33d72120c19d,
            CommandAction::OpenGui => 0x44e23c12_5883_4903_9da9_33d72120c19e,
        };
        Ok(GUID::from_u128(u128_id))
    }

    fn GetState(
        &self,
        _psiitemarray: Option<&IShellItemArray>,
        _foktousetickedstate: BOOL,
    ) -> windows::core::Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(
        &self,
        psiitemarray: Option<&IShellItemArray>,
        _pbc: Option<&IBindCtx>,
    ) -> windows::core::Result<()> {
        let paths = get_selected_paths(psiitemarray);
        if paths.is_empty() {
            return Ok(());
        }

        let exe = get_bayzip_exe_path();
        let mut cmd = std::process::Command::new(exe);

        match self.action {
            CommandAction::Root | CommandAction::OpenGui => {
                let is_all_archives = !paths.is_empty() && paths.iter().all(|p| {
                    let path = std::path::Path::new(p);
                    if path.is_dir() {
                        return false;
                    }
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                    matches!(
                        ext.as_str(),
                        "bay" | "bayg" | "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "iso" | "cab"
                    )
                });

                if is_all_archives {
                    cmd.arg("--gui-extract");
                } else {
                    cmd.arg("--gui-compress");
                }
                for p in &paths {
                    cmd.arg(p);
                }
            }
            CommandAction::CompressGamer => {
                cmd.arg("--quick-compress");
                cmd.arg(".bayg");
                for p in &paths {
                    cmd.arg(p);
                }
            }
            CommandAction::CompressBay => {
                cmd.arg("--quick-compress");
                cmd.arg(".bay");
                for p in &paths {
                    cmd.arg(p);
                }
            }
            CommandAction::ExtractHere => {
                cmd.arg("--extract-here");
                for p in &paths {
                    cmd.arg(p);
                }
            }
        }

        let _ = cmd.spawn();
        Ok(())
    }

    fn GetFlags(&self) -> windows::core::Result<u32> {
        if self.action == CommandAction::Root {
            Ok(ECF_HASSUBCOMMANDS.0 as u32)
        } else {
            Ok(ECF_DEFAULT.0 as u32)
        }
    }

    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        if self.action == CommandAction::Root {
            let sub_commands = vec![
                BayzipExplorerCommand::new(CommandAction::CompressGamer).into(),
                BayzipExplorerCommand::new(CommandAction::CompressBay).into(),
                BayzipExplorerCommand::new(CommandAction::ExtractHere).into(),
                BayzipExplorerCommand::new(CommandAction::OpenGui).into(),
            ];
            Ok(BayzipEnumExplorerCommand::new(sub_commands).into())
        } else {
            Err(windows::core::Error::from_hresult(E_NOTIMPL))
        }
    }
}

#[implement(IEnumExplorerCommand)]
pub struct BayzipEnumExplorerCommand {
    commands: Vec<IExplorerCommand>,
    index: AtomicUsize,
}

impl BayzipEnumExplorerCommand {
    pub fn new(commands: Vec<IExplorerCommand>) -> Self {
        OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
        Self {
            commands,
            index: AtomicUsize::new(0),
        }
    }
}

impl Drop for BayzipEnumExplorerCommand {
    fn drop(&mut self) {
        OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IEnumExplorerCommand_Impl for BayzipEnumExplorerCommand_Impl {
    fn Next(
        &self,
        celt: u32,
        ppelem: *mut Option<IExplorerCommand>,
        pcfetched: *mut u32,
    ) -> HRESULT {
        if ppelem.is_null() {
            return E_POINTER;
        }

        let mut fetched = 0u32;
        let mut idx = self.index.load(Ordering::SeqCst);

        while fetched < celt && idx < self.commands.len() {
            unsafe {
                *ppelem.add(fetched as usize) = Some(self.commands[idx].clone());
            }
            fetched += 1;
            idx += 1;
        }

        self.index.store(idx, Ordering::SeqCst);

        if !pcfetched.is_null() {
            unsafe { *pcfetched = fetched };
        }

        if fetched == celt {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> windows::core::Result<()> {
        let current = self.index.load(Ordering::SeqCst);
        let next = (current + celt as usize).min(self.commands.len());
        self.index.store(next, Ordering::SeqCst);
        Ok(())
    }

    fn Reset(&self) -> windows::core::Result<()> {
        self.index.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IEnumExplorerCommand> {
        let cloned = BayzipEnumExplorerCommand::new(self.commands.clone());
        cloned.index.store(self.index.load(Ordering::SeqCst), Ordering::SeqCst);
        Ok(cloned.into())
    }
}

#[implement(IClassFactory)]
pub struct BayzipClassFactory;

impl IClassFactory_Impl for BayzipClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut std::ffi::c_void,
    ) -> windows::core::Result<()> {
        if punkouter.is_some() {
            return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::CLASS_E_NOAGGREGATION));
        }
        if ppvobject.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }

        let command: IExplorerCommand = BayzipExplorerCommand::new(CommandAction::Root).into();
        unsafe { command.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, flock: BOOL) -> windows::core::Result<()> {
        if flock.as_bool() {
            OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
        } else {
            OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();

    if unsafe { *rclsid } != CLSID_BAYZIP_SHELL {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    let factory: IClassFactory = BayzipClassFactory.into();
    unsafe { factory.query(riid, ppv).into() }
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECT_COUNT.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
