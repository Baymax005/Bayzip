use windows::core::{implement, HRESULT};
use windows::Win32::UI::Shell::{IExplorerCommand, IShellItemArray, IEnumExplorerCommand};
use windows::Win32::Foundation::{E_NOTIMPL, S_OK, BOOL};

// Main Menu Item "Bayzip"
#[implement(IExplorerCommand)]
struct BayzipMainMenu;

impl IExplorerCommand_Impl for BayzipMainMenu {
    fn Title(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<windows::core::PWSTR> {
        Ok(windows::core::w!("Bayzip").into())
    }

    fn Icon(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<windows::core::PWSTR> {
        Ok(windows::core::PWSTR::null()) // Path to bayzip.ico
    }

    fn ToolTip(&self, _psiitemarray: Option<&IShellItemArray>) -> windows::core::Result<windows::core::PWSTR> {
        Ok(windows::core::PWSTR::null())
    }

    fn State(&self, _psiitemarray: Option<&IShellItemArray>, _foktobeobsolete: BOOL) -> windows::core::Result<u32> {
        Ok(0) // ECS_ENABLED
    }

    fn Invoke(&self, _psiitemarray: Option<&IShellItemArray>, _pbc: Option<&windows::Win32::System::Com::IBindCtx>) -> windows::core::Result<()> {
        // If clicked without submenu, open the Bayzip UI
        Ok(())
    }

    fn Flags(&self) -> windows::core::Result<u32> {
        // ECF_HASSUBCOMMANDS (0x20) indicates this item opens a dropdown sub-menu
        Ok(0x00000020) 
    }

    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        // Here we return an enumerator for the Sub-Menu containing the standard options
        // This allows the user to skip opening the app and select these directly:
        // 1. "Compress to .bay (Smart Profile)"
        // 2. "Compress to .zip"
        // 3. "Compress to .7z"
        Err(windows::core::Error::from(E_NOTIMPL))
    }
}
