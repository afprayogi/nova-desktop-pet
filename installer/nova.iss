; Pemasang NOVA (Inno Setup 6). Dibangun lewat installer\bangun.ps1 atau workflow rilis.
; Dipasang per-pengguna (tanpa admin) di %LOCALAPPDATA%\Programs\NOVA. Data pengguna ada di %APPDATA%\nova dan tidak disentuh pembaruan.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{B7C6A2B4-6C1E-4E52-9B55-6F0E5A3D7A11}
AppName=NOVA
AppVersion={#AppVersion}
AppVerName=NOVA {#AppVersion}
AppPublisher=afprayogi
AppPublisherURL=https://github.com/afprayogi/nova-desktop-pet
DefaultDirName={localappdata}\Programs\NOVA
DefaultGroupName=NOVA
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=NOVA-Setup-{#AppVersion}
SetupIconFile=nova.ico
UninstallDisplayIcon={app}\nova.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
UsePreviousTasks=yes
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "autostart"; Description: "Jalankan NOVA otomatis saat masuk Windows"; Flags: checkedonce
Name: "desktopicon"; Description: "Buat ikon di desktop"; Flags: unchecked

[Files]
Source: "..\target\release\nova.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\NOVA"; Filename: "{app}\nova.exe"
Name: "{autoprograms}\Hapus NOVA"; Filename: "{uninstallexe}"
Name: "{autodesktop}\NOVA"; Filename: "{app}\nova.exe"; Tasks: desktopicon

[Run]
; Nyala otomatis lewat Windows Task Scheduler (tugas "NOVA\Mulai": saat login + dihidupkan lagi bila mati). Tanpa admin.
Filename: "{app}\nova.exe"; Parameters: "--autostart on"; Flags: runhidden waituntilterminated; Tasks: autostart
; Tanpa skipifsilent: setelah pembaruan senyap, NOVA menyala lagi sendiri.
Filename: "{app}\nova.exe"; Description: "Jalankan NOVA sekarang"; Flags: nowait postinstall

[UninstallRun]
Filename: "{app}\nova.exe"; Parameters: "--autostart off"; Flags: runhidden waituntilterminated; RunOnceId: "HapusTugasMulai"

[Code]
procedure TutupNova();
var
  R: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM nova.exe', '', SW_HIDE, ewWaitUntilTerminated, R);
  Sleep(800);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  TutupNova();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    TutupNova();
  if CurUninstallStep = usPostUninstall then
  begin
    { Data pribadi (memori, kunci terenkripsi, catatan) hanya dihapus bila pengguna setuju; bawaan: TIDAK. }
    if (not UninstallSilent) and
       (MsgBox('Hapus juga data NOVA (memori, catatan, kunci tersimpan) di folder pengguna?' + #13#10 + 'Pilih Tidak untuk menyimpannya.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
      DelTree(ExpandConstant('{userappdata}\nova'), True, True, True);
  end;
end;
