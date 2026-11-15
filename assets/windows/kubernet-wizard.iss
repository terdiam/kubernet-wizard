; Inno Setup script for the kubernet-wizard Windows installer.
; Build: iscc /DAppVersion=0.1.2 /DSourceDir=dist-in /DOutputDir=dist assets\windows\kubernet-wizard.iss
;
; Installs per user (no admin) into %LOCALAPPDATA%\Programs so the in-app
; auto-updater can replace the executable without elevation.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "."
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{6F2B8C1E-4D7A-4B9E-9C3F-2A1D5E8B7C40}
AppName=kubernet-wizard
AppVersion={#AppVersion}
AppVerName=kubernet-wizard {#AppVersion}
AppPublisher=kubernet-wizard
AppPublisherURL=https://github.com/terdiam/kubernet-wizard
AppSupportURL=https://github.com/terdiam/kubernet-wizard/issues
DefaultDirName={localappdata}\Programs\kubernet-wizard
DefaultGroupName=kubernet-wizard
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icon\kubernet-wizard.ico
UninstallDisplayIcon={app}\kubernet-wizard.exe
OutputDir={#OutputDir}
OutputBaseFilename=kubernet-wizard-setup-windows-x86_64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addtopath"; Description: "Add the kw CLI to PATH"; Flags: unchecked

[Files]
Source: "{#SourceDir}\kubernet-wizard.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\kw.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"

[Icons]
Name: "{autoprograms}\kubernet-wizard"; Filename: "{app}\kubernet-wizard.exe"
Name: "{autodesktop}\kubernet-wizard"; Filename: "{app}\kubernet-wizard.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\kubernet-wizard.exe"; Description: "{cm:LaunchProgram,kubernet-wizard}"; Flags: nowait postinstall skipifsilent

[Code]
function NeedsAddPath(Dir: string): Boolean;
var
  Paths: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Paths, Dir: string;
  P: Integer;
begin
  if CurUninstallStep <> usPostUninstall then
    exit;
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths) then
    exit;
  Dir := ';' + ExpandConstant('{app}');
  P := Pos(Uppercase(Dir), Uppercase(Paths));
  if P > 0 then
  begin
    Delete(Paths, P, Length(Dir));
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths);
  end;
end;
