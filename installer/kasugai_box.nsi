Unicode true
ManifestDPIAware true

!include MUI2.nsh
!include x64.nsh

Name "kasugai_box"
OutFile "..\download\kasugai_box_setup.exe"
InstallDir "C:\kasugai\kasugai_box"
RequestExecutionLevel admin

!define MUI_ABORTWARNING

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Japanese"

Section "kasugai_box" SecMain
  SetOutPath $INSTDIR
  File "..\server\target\release\kasugai_box.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\kasugai_box"
  CreateShortcut "$SMPROGRAMS\kasugai_box\kasugai_box.lnk" "$INSTDIR\kasugai_box.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\kasugai_box.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\kasugai_box\kasugai_box.lnk"
  RMDir "$SMPROGRAMS\kasugai_box"
  RMDir "$INSTDIR"
SectionEnd
