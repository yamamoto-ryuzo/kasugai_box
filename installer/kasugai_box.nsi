Unicode true
ManifestDPIAware true

!include MUI2.nsh
!include x64.nsh

Name "kasugai_box"
OutFile "..\download\kasugai_box_setup.exe"
InstallDir "C:\kasugai\kasugai_box"
RequestExecutionLevel user

!define MUI_ABORTWARNING
; icon.ico は 16x16 から 256x256 までのマルチサイズ ICO を含む
!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish page options
!define MUI_FINISHPAGE_RUN "$INSTDIR\kasugai_box.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--open-browser"
!define MUI_FINISHPAGE_RUN_TEXT "Run kasugai"
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Create desktop shortcut"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcut
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Japanese"

Section "kasugai_box" SecMain
  SetOutPath $INSTDIR
  File "..\server\target\release\kasugai_box.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\kasugai_box"
  CreateShortcut "$SMPROGRAMS\kasugai_box\kasugai_box.lnk" "$INSTDIR\kasugai_box.exe" "--open-browser" "$INSTDIR\kasugai_box.exe" 0 SW_SHOWNORMAL "" "" "$INSTDIR"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\kasugai_box.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\kasugai_box\kasugai_box.lnk"
  Delete "$DESKTOP\kasugai_box.lnk"
  RMDir "$SMPROGRAMS\kasugai_box"
  RMDir "$INSTDIR"
SectionEnd

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\kasugai_box.lnk" "$INSTDIR\kasugai_box.exe" "--open-browser" "$INSTDIR\kasugai_box.exe" 0 SW_SHOWNORMAL "" "" "$INSTDIR"
FunctionEnd
