// Windows Taskbar Jump List and System Menu integration for Nebulae.
// Configures explicit AppUserModelID and registers Jump List tasks (such as "Quit Nebulae")
// so that right-clicking the application icon on the Windows bottom bar (taskbar)
// provides a direct option to quit or restore the application.

#[cfg(target_os = "windows")]
use windows::core::{Interface, w, GUID, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
#[cfg(target_os = "windows")]
use windows::Win32::System::Variant::VARENUM;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::PROPERTYKEY;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
    SetCurrentProcessExplicitAppUserModelID, ShellLink, SHStrDupW,
};

#[cfg(target_os = "windows")]
pub const APP_ID: PCWSTR = w!("org.asiandvdclub.nebulae");

// PKEY_Title = {F29F85E0-4FF9-1068-AB91-08002B27B3D9}, 2
#[cfg(target_os = "windows")]
const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xF29F85E0_4FF9_1068_AB91_08002B27B3D9),
    pid: 2,
};

/// Sets the explicit AppUserModelID for the current process on Windows.
/// This ensures taskbar grouping and custom Jump Lists bind to the correct taskbar icon.
pub fn init_taskbar_app_id() {
    #[cfg(target_os = "windows")]
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(APP_ID);
    }
}

/// Populates the Windows Taskbar Jump List with tasks, including "Quit Nebulae" and "Open Nebulae".
/// When the user right-clicks the application icon on the bottom bar (taskbar), these tasks
/// appear under the "Tasks" category.
pub fn update_taskbar_jump_list() {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(|| {
            if let Err(e) = populate_jump_list_inner() {
                tracing::warn!("[taskbar] failed to update Jump List: {e}");
            } else {
                tracing::info!("[taskbar] Windows Taskbar Jump List successfully updated with Quit option");
            }
        });
    }
}

#[cfg(target_os = "windows")]
fn populate_jump_list_inner() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let exe_path_str = current_exe.to_string_lossy().to_string();
    let exe_wide: Vec<u16> = exe_path_str.encode_utf16().chain(std::iter::once(0)).collect();
    let exe_pcwstr = PCWSTR::from_raw(exe_wide.as_ptr());

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dest_list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        dest_list.SetAppID(APP_ID)?;

        let mut min_slots = 0u32;
        let _removed: IObjectArray = dest_list.BeginList(&mut min_slots)?;

        let collection: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;

        // 1. Task: "Quit Nebulae" (runs current_exe --quit)
        {
            let quit_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            quit_link.SetPath(exe_pcwstr)?;
            quit_link.SetArguments(w!("--quit"))?;
            quit_link.SetIconLocation(exe_pcwstr, 0)?;
            quit_link.SetDescription(w!("Quit Nebulae completely"))?;

            let prop_store: IPropertyStore = quit_link.cast()?;
            let pwstr = SHStrDupW(w!("Quit Nebulae"))?;
            let mut pv = PROPVARIANT::default();
            (*pv.Anonymous.Anonymous).vt = VARENUM(31); // VT_LPWSTR
            (*pv.Anonymous.Anonymous).Anonymous.pwszVal = pwstr;
            prop_store.SetValue(&PKEY_TITLE, &pv)?;
            prop_store.Commit()?;
            let _ = PropVariantClear(&mut pv);

            collection.AddObject(&quit_link)?;
        }

        // 2. Task: "Open Nebulae" (runs current_exe without arguments, focusing the main window)
        {
            let open_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            open_link.SetPath(exe_pcwstr)?;
            open_link.SetArguments(w!(""))?;
            open_link.SetIconLocation(exe_pcwstr, 0)?;
            open_link.SetDescription(w!("Open Nebulae window"))?;

            let prop_store: IPropertyStore = open_link.cast()?;
            let pwstr = SHStrDupW(w!("Open Nebulae"))?;
            let mut pv = PROPVARIANT::default();
            (*pv.Anonymous.Anonymous).vt = VARENUM(31); // VT_LPWSTR
            (*pv.Anonymous.Anonymous).Anonymous.pwszVal = pwstr;
            prop_store.SetValue(&PKEY_TITLE, &pv)?;
            prop_store.Commit()?;
            let _ = PropVariantClear(&mut pv);

            collection.AddObject(&open_link)?;
        }

        let tasks_array: IObjectArray = collection.cast()?;
        dest_list.AddUserTasks(&tasks_array)?;
        dest_list.CommitList()?;
    }

    Ok(())
}
