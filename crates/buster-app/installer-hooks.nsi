!macro customInit
  ; 1. Kill any active processes from legacy C# or Rust versions
  nsExec::Exec 'taskkill /F /IM buster-app.exe /IM BusterVPN.exe /IM xray.exe /IM hysteria.exe /T'
  Sleep 500

  ; 2. Wipe stale WebView2 user data (fixes language / theme / localStorage corruption)
  RMDir /r "$LOCALAPPDATA\BusterVPN\EBWebView"
  RMDir /r "$LOCALAPPDATA\app.bustervpn.client\EBWebView"

  ; 3. Clean legacy runtime caches, logs, journals and legacy configs
  RMDir /r "$LOCALAPPDATA\BusterVPN\runtime"
  Delete "$LOCALAPPDATA\BusterVPN\*.log"
  Delete "$LOCALAPPDATA\BusterVPN\*.txt"
  Delete "$LOCALAPPDATA\BusterVPN\*.config"
  Delete "$LOCALAPPDATA\BusterVPN\*.xml"
  Delete "$LOCALAPPDATA\BusterVPN\*.tmp"
  Delete "$LOCALAPPDATA\BusterVPN\*.bak"
  Delete "$LOCALAPPDATA\BusterVPN\session.json"
  Delete "$LOCALAPPDATA\BusterVPN\proxy_journal.json"
  Delete "$LOCALAPPDATA\BusterVPN\killswitch_journal.json"
!macroend

!macro customInstall
  ; 4. If legacy Inno Setup uninstaller exists in $INSTDIR or Program Files, execute it silently
  ${If} ${FileExists} "$INSTDIR\unins000.exe"
    ExecWait '"$INSTDIR\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
    Sleep 1000
  ${EndIf}
  ${If} ${FileExists} "$PROGRAMFILES64\BusterVPN\unins000.exe"
    ExecWait '"$PROGRAMFILES64\BusterVPN\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
    Sleep 1000
  ${EndIf}

  ; 5. Clean Inno Setup uninstall registry entries
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\BusterVPN_is1"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BusterVPN_is1"
  DeleteRegKey HKLM "Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\BusterVPN_is1"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\BusterVPN"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BusterVPN"

  ; 6. Purge all legacy .NET 8 / C# binaries and configs from $INSTDIR
  Delete "$INSTDIR\BusterVPN.exe"
  Delete "$INSTDIR\BusterVPN.*"
  Delete "$INSTDIR\*.deps.json"
  Delete "$INSTDIR\*.runtimeconfig.json"
  Delete "$INSTDIR\*.pdb"
  Delete "$INSTDIR\unins000.*"
  Delete "$INSTDIR\wpfgfx_cor3.dll"
  Delete "$INSTDIR\PresentationNative_cor3.dll"
  Delete "$INSTDIR\clr*.dll"
  Delete "$INSTDIR\coreclr.dll"
  Delete "$INSTDIR\System.*.dll"
  Delete "$INSTDIR\Microsoft.*.dll"
  Delete "$INSTDIR\mscordaccore.dll"
  Delete "$INSTDIR\mscorrc.dll"
  Delete "$INSTDIR\hostfxr.dll"
  Delete "$INSTDIR\hostpolicy.dll"
  Delete "$INSTDIR\*.ico"
  RMDir /r "$INSTDIR\cores"
  RMDir /r "$INSTDIR\runtimes"
!macroend

!macro customUnInstall
  ; Complete cleanup on uninstallation
  RMDir /r "$LOCALAPPDATA\BusterVPN"
  RMDir /r "$LOCALAPPDATA\app.bustervpn.client"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro customInit
  !insertmacro customInstall
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro customUnInstall
!macroend
