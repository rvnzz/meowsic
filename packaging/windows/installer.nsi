; meowsic — NSIS installer
; VERSION передаётся через /DVERSION=x.y.z

!ifndef VERSION
  !define VERSION "0.0.0"
!endif

Unicode true
ManifestDPIAware true

Name "meowsic ${VERSION}"
OutFile "meowsic-${VERSION}-win64-setup.exe"
InstallDir "$PROGRAMFILES64\meowsic"
InstallDirRegKey HKLM "Software\meowsic" "Install_Dir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"

Section "meowsic" SecApp
  SetOutPath "$INSTDIR"
  File "..\..\target\release\meowsic.exe"

  CreateDirectory "$SMPROGRAMS\meowsic"
  CreateShortCut "$SMPROGRAMS\meowsic\meowsic.lnk" "$INSTDIR\meowsic.exe"
  CreateShortCut "$DESKTOP\meowsic.lnk" "$INSTDIR\meowsic.exe"

  WriteRegStr HKLM "Software\meowsic" "Install_Dir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "DisplayName" "meowsic"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "Publisher" "rvnzz"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic" "NoRepair" 1
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\meowsic.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\meowsic\meowsic.lnk"
  RMDir "$SMPROGRAMS\meowsic"
  Delete "$DESKTOP\meowsic.lnk"
  DeleteRegKey HKLM "Software\meowsic"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\meowsic"
  ; настройки пользователя не трогаем: %APPDATA%\meowsic
SectionEnd
