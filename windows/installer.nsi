; Wayfarer NSIS installer script
; Produces a single self-contained setup exe — no admin rights required.
; Run from the repository root:
;   makensis windows\installer.nsi

Unicode True
!include "MUI2.nsh"

; ── Metadata ─────────────────────────────────────────────────────────────────
Name "Wayfarer"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Wayfarer"
InstallDirRegKey HKCU "Software\Wayfarer" "InstallDir"
RequestExecutionLevel user   ; no UAC prompt needed

; ── Pages ─────────────────────────────────────────────────────────────────────
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ── Install ───────────────────────────────────────────────────────────────────
Section "Wayfarer" SecMain
    SetOutPath "$INSTDIR"

    ; Copy everything from the bundle directory (path passed as /D define from CI)
    File /r "${BUNDLE_DIR}\*.*"

    ; Write uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKCU "Software\Wayfarer" "InstallDir" "$INSTDIR"

    ; Desktop shortcut
    CreateShortCut "$DESKTOP\Wayfarer.lnk" "$INSTDIR\wayfarer.exe"

    ; Start menu
    CreateDirectory "$SMPROGRAMS\Wayfarer"
    CreateShortCut "$SMPROGRAMS\Wayfarer\Wayfarer.lnk" "$INSTDIR\wayfarer.exe"
    CreateShortCut "$SMPROGRAMS\Wayfarer\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

; ── Uninstall ─────────────────────────────────────────────────────────────────
Section "Uninstall"
    RMDir /r "$INSTDIR"
    Delete "$DESKTOP\Wayfarer.lnk"
    RMDir /r "$SMPROGRAMS\Wayfarer"
    DeleteRegKey HKCU "Software\Wayfarer"
SectionEnd
