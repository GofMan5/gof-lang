#ifndef AppVersion
  #define AppVersion "dev"
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
AppId={{2B2468CE-2D46-4DA3-9B3E-A7DB98E6E89B}
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

procedure AddToUserPath(const Segment: string);
var
  PathValue: string;
begin
  if not RegQueryStringValue(HKCU, EnvironmentSubkey, PathValueName, PathValue) then
    PathValue := '';

  if not PathContainsSegment(PathValue, Segment) then
  begin
    if PathValue = '' then
      PathValue := Segment
    else
      PathValue := PathValue + ';' + Segment;

    RegWriteExpandStringValue(HKCU, EnvironmentSubkey, PathValueName, PathValue);
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
begin
  if CurStep = ssPostInstall then
  begin
    SaveStringToFile(ExpandConstant('{app}\VERSION.txt'), '{#AppVersion}' + #13#10, False);
    AddToUserPath(ExpandConstant('{app}\bin'));
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromUserPath(ExpandConstant('{app}\bin'));
end;
