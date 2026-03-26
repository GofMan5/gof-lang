#ifndef AppVersion
  #define AppVersion "dev"
#endif

#ifndef AppIdValue
  #define AppIdValue "{{2B2468CE-2D46-4DA3-9B3E-A7DB98E6E89B}"
#endif

#ifndef CreateUninstallRegKeyValue
  #define CreateUninstallRegKeyValue "yes"
#endif

#ifndef PayloadDir
  #error PayloadDir must point at the Windows installer payload directory.
#endif

#ifndef OutputDir
  #define OutputDir "."
#endif

#ifndef OutputBaseFilename
  #define OutputBaseFilename "gof-windows-x86_64-setup"
#endif

[Setup]
AppId={#AppIdValue}
AppName=gof
AppVersion={#AppVersion}
AppVerName=gof {#AppVersion}
AppPublisher=GofMan5
AppPublisherURL=https://github.com/GofMan5/gof-lang
AppSupportURL=https://github.com/GofMan5/gof-lang/issues
AppUpdatesURL=https://github.com/GofMan5/gof-lang/releases
DefaultDirName={localappdata}\Programs\gof
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
LicenseFile={#PayloadDir}\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseFilename}
UninstallDisplayIcon={app}\bin\gof.exe
ChangesEnvironment=yes
CreateUninstallRegKey={#CreateUninstallRegKeyValue}
SetupLogging=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#PayloadDir}\gof.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#PayloadDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Code]
const
  PathValueName = 'Path';
  EnvironmentSubkey = 'Environment';
  PostInstallNotesName = 'POST_INSTALL_NOTES.txt';

function PathContainsSegment(const PathValue: string; const Segment: string): Boolean;
begin
  Result := Pos(';' + Uppercase(Segment) + ';', ';' + Uppercase(PathValue) + ';') > 0;
end;

function RemovePathSegment(const PathValue: string; const Segment: string): string;
var
  Remaining: string;
  DelimiterPos: Integer;
  Item: string;
begin
  Result := '';
  Remaining := PathValue;

  while Remaining <> '' do
  begin
    DelimiterPos := Pos(';', Remaining);
    if DelimiterPos = 0 then
    begin
      Item := Trim(Remaining);
      Remaining := '';
    end
    else
    begin
      Item := Trim(Copy(Remaining, 1, DelimiterPos - 1));
      Delete(Remaining, 1, DelimiterPos);
    end;

    if (Item <> '') and (CompareText(Item, Segment) <> 0) then
    begin
      if Result <> '' then
        Result := Result + ';';
      Result := Result + Item;
    end;
  end;
end;

function PostInstallNotesPath: string;
begin
  Result := ExpandConstant('{app}\' + PostInstallNotesName);
end;

procedure ClearPostInstallNotes();
begin
  if FileExists(PostInstallNotesPath()) then
    DeleteFile(PostInstallNotesPath());
end;

procedure WritePathManualSetupNotes(const Segment: string);
var
  Notes: string;
begin
  Notes :=
    'gof was installed successfully, but Setup could not update your user PATH.' + #13#10 + #13#10 +
    'Add this directory to PATH manually:' + #13#10 +
    Segment + #13#10 + #13#10 +
    'After updating PATH, open a new terminal and run: gof --help' + #13#10;

  SaveStringToFile(PostInstallNotesPath(), Notes, False);
end;

function AddToUserPath(const Segment: string): Boolean;
var
  PathValue: string;
begin
  Result := True;

  if not RegQueryStringValue(HKCU, EnvironmentSubkey, PathValueName, PathValue) then
    PathValue := '';

  if not PathContainsSegment(PathValue, Segment) then
  begin
    if PathValue = '' then
      PathValue := Segment
    else
      PathValue := PathValue + ';' + Segment;

    Result := RegWriteExpandStringValue(HKCU, EnvironmentSubkey, PathValueName, PathValue);
  end;
end;

procedure RemoveFromUserPath(const Segment: string);
var
  CurrentPath: string;
  NextPath: string;
begin
  if not RegQueryStringValue(HKCU, EnvironmentSubkey, PathValueName, CurrentPath) then
    exit;

  NextPath := RemovePathSegment(CurrentPath, Segment);

  if NextPath = '' then
    RegDeleteValue(HKCU, EnvironmentSubkey, PathValueName)
  else
    RegWriteExpandStringValue(HKCU, EnvironmentSubkey, PathValueName, NextPath);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  BinDir: string;
begin
  if CurStep = ssPostInstall then
  begin
    SaveStringToFile(ExpandConstant('{app}\VERSION.txt'), '{#AppVersion}' + #13#10, False);
    ClearPostInstallNotes();

    BinDir := ExpandConstant('{app}\bin');
    if not AddToUserPath(BinDir) then
    begin
      Log('Unable to update the user PATH. Writing manual setup notes.');
      WritePathManualSetupNotes(BinDir);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromUserPath(ExpandConstant('{app}\bin'));
end;