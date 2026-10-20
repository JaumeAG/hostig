#[cfg(unix)]
pub fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(windows)]
pub fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let _ = CloseHandle(handle);
        true
    }
}

#[cfg(windows)]
pub fn restrict_to_user_and_system(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };

    let sid = current_user_sid()?;
    let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;{sid})");
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(io_err)?;
        let path_wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = SetFileSecurityW(
            PCWSTR(path_wide.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        result.ok().map_err(io_err)?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn current_user_sid() -> std::io::Result<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::TOKEN_QUERY;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(io_err)?;
        let sid = sid_of_token(token);
        let _ = CloseHandle(token);
        sid
    }
}

#[cfg(windows)]
pub fn process_user_sid(pid: u32) -> std::io::Result<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::TOKEN_QUERY;
    use windows::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(io_err)?;
        let mut token = windows::Win32::Foundation::HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        let _ = CloseHandle(process);
        opened.map_err(io_err)?;
        let sid = sid_of_token(token);
        let _ = CloseHandle(token);
        sid
    }
}

#[cfg(windows)]
fn sid_of_token(token: windows::Win32::Foundation::HANDLE) -> std::io::Result<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_USER};
    unsafe {
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 {
            return Err(std::io::Error::other(
                "GetTokenInformation no devolvió tamaño",
            ));
        }
        let mut buffer = vec![0u8; needed as usize];
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut _),
            needed,
            &mut needed,
        )
        .map_err(io_err)?;
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(io_err)?;
        let sid = text
            .to_string()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(text.0 as *mut _)));
        Ok(sid)
    }
}

#[cfg(windows)]
pub fn pipe_sddl() -> std::io::Result<String> {
    let sid = current_user_sid()?;
    Ok(format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})"))
}

#[cfg(windows)]
fn io_err(error: windows::core::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn la_dacl_del_pipe_es_el_usuario_y_system() {
        let sddl = super::pipe_sddl().unwrap();
        assert!(sddl.contains("(A;;GA;;;SY)"));
        assert!(sddl.contains(&super::current_user_sid().unwrap()));
        assert!(!sddl.contains("WD"));
        assert!(!sddl.contains("AU"));
    }
}
