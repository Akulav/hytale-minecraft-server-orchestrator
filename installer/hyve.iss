; Hyve Windows setup (Inno Setup 6): Welcome, License, Folder, Shortcuts (desktop icon),
; Install, Finish ("Launch Hyve"). Per user, no administrator rights, with an uninstaller.
; The Build workflow compiles it:
;
;   iscc /DAppVersion=0.1.0 installer\hyve.iss
;
; Updates: this setup is also the update. Hyve's in-app updater (src/updater.rs) downloads it
; and runs it on the install folder:
;
;   Hyve-setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /LAUNCH=1 /DIR=<folder> /LOG=<file>
;
; Run by hand over an existing install it finds the previous folder by itself (same AppId +
; UsePreviousAppDir), stops a running Hyve from that folder, replaces the program and keeps the
; data (instances, worlds, backups in "data" next to the program).
;
; Hyve was called HyOrchestrator before 0.4. The AppId is unchanged, so this setup updates such an install
; in place: same folder and data, the Apps entry is renamed, and the old hyorchestrator.exe and its
; shortcuts are removed ([InstallDelete]).

#ifndef AppVersion
  ; scripts\bump-version.ps1 keeps this in sync with Cargo.toml
  #define AppVersion "0.6.0"
#endif
#ifndef Exe
  #define Exe "..\target\release\hyve.exe"
#endif
#ifndef Name
  #define Name "Hyve"
#endif
#ifndef OldName
  ; the name before 0.4, for removing its shortcuts (tests pass a throwaway name)
  #define OldName "HyOrchestrator"
#endif
#ifndef Guid
  ; never change it: it is how an update or the uninstaller finds this install
  #define Guid "4E1B7C2A-9D3F-4A6B-8C55-2F0E7D9A31B4"
#endif
; (Windows file versions are numbers only: 1.0.0-beta.1 -> 1.0.0)
#define NumVersion Copy(AppVersion, 1, Pos("-", AppVersion + "-") - 1)

[Setup]
AppId={{{#Guid}}
AppName={#Name}
AppVersion={#AppVersion}
AppVerName={#Name} {#AppVersion}
AppPublisher=Akulav
AppCopyright=Made by Akulav
AppPublisherURL=https://github.com/Akulav/hytale-server-orchestrator
AppSupportURL=https://github.com/Akulav/hytale-server-orchestrator/issues
AppUpdatesURL=https://github.com/Akulav/hytale-server-orchestrator/releases
VersionInfoCompany=Akulav
VersionInfoCopyright=Made by Akulav
VersionInfoProductName={#Name}
VersionInfoDescription={#Name} Setup
VersionInfoVersion={#NumVersion}
DefaultDirName={localappdata}\{#Name}
; the folder page is shown so the install location can be chosen; an update reuses the previous one
DisableWelcomePage=no
DisableDirPage=no
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
DirExistsWarning=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
OutputDir=Output
OutputBaseFilename=Hyve-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\hyve.exe
UninstallDisplayName={#Name}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=force
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
AppComment=Run and manage Hytale and Minecraft servers - made by Akulav
DeleteData=Hyve is uninstalled.%n%nAlso delete its data in %1 ?%nThat deletes every server instance, world, mod, downloaded server version and backup for good.%n%nNo keeps them: installing Hyve again brings everything back.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "hyve.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#Name}"; Filename: "{app}\hyve.exe"; WorkingDir: "{app}"; Comment: "{cm:AppComment}"
Name: "{autoprograms}\{#Name} (headless web UI)"; Filename: "{app}\hyve.exe"; Parameters: "--headless"; WorkingDir: "{app}"; Comment: "Serve the web UI without a window"
Name: "{autodesktop}\{#Name}"; Filename: "{app}\hyve.exe"; WorkingDir: "{app}"; Comment: "{cm:AppComment}"; Tasks: desktopicon

[Run]
Filename: "{app}\hyve.exe"; Description: "{cm:LaunchProgram,{#Name}}"; Flags: nowait postinstall skipifsilent
; an update started by the in-app updater: start the new version when the silent install is done
Filename: "{app}\hyve.exe"; Parameters: "--after-update"; WorkingDir: "{app}"; Flags: nowait; Check: LaunchAfterUpdate

[InstallDelete]
; left over from HyOrchestrator (before 0.4)
Type: files; Name: "{app}\hyorchestrator.exe"
Type: files; Name: "{app}\hyorchestrator.exe.old"
Type: files; Name: "{autoprograms}\{#OldName}.lnk"
Type: files; Name: "{autoprograms}\{#OldName} (headless web UI).lnk"
Type: files; Name: "{autodesktop}\{#OldName}.lnk"

[UninstallDelete]
Type: files; Name: "{app}\hyve.exe.old"

[Code]
// The data folder (instances, worlds, versions, backups): "data" next to the program, in the folder
// chosen on the folder page. (Before 0.3 it was always %LOCALAPPDATA%\HyOrchestrator\data; the program
// moves it here on its first start when both are on the same drive.)
function DataDir(): String;
begin
  Result := ExpandConstant('{app}\data');
  if not DirExists(Result) then
    Result := ExpandConstant('{localappdata}\{#Name}\data');
  if not DirExists(Result) then
    Result := ExpandConstant('{localappdata}\HyOrchestrator\data');
end;

// A running Hyve (or HyOrchestrator, before 0.4) from this folder is asked to close, then ended (it stops
// its servers first when closed normally; the in-app updater has already stopped them).
procedure StopRunning(Dir: String);
var
  Code: Integer;
begin
  Exec('powershell.exe',
       '-NoProfile -NonInteractive -Command "$p = Get-Process hyve, hyorchestrator -ErrorAction SilentlyContinue | ' +
       'Where-Object { $_.Path -eq ''' + Dir + '\hyve.exe'' -or $_.Path -eq ''' + Dir + '\hyorchestrator.exe'' }; ' +
       '$p | ForEach-Object { $_.CloseMainWindow() | Out-Null }; ' +
       'if ($p) { Start-Sleep -Seconds 8 }; $p | Where-Object { -not $_.HasExited } | Stop-Process -Force"',
       '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function LaunchAfterUpdate(): Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:LAUNCH|0}') = '1');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  StopRunning(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    StopRunning(ExpandConstant('{app}'));
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
  begin
    if DirExists(DataDir()) and (MsgBox(FmtMessage(CustomMessage('DeleteData'), [DataDir()]), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    begin
      DelTree(DataDir(), True, True, True);
      RemoveDir(ExpandConstant('{app}'));
    end;
  end;
end;
