@echo off
rem Memasang NOVA sebagai asisten yang otomatis muncul saat laptop menyala (hanya untuk pengguna ini, tanpa admin).
rem Menyalin program ke %LOCALAPPDATA%\NOVA lalu mendaftarkan entri startup HKCU\...\Run. Batalkan: lepas-asisten.bat
cd /d "%~dp0"
if not exist target\release\nova.exe (
  echo Membangun NOVA ^(sekali saja, beberapa menit^)...
  cargo build --release || goto :err
)
mkdir "%LOCALAPPDATA%\NOVA" 2>nul
copy /y target\release\nova.exe "%LOCALAPPDATA%\NOVA\nova.exe" >nul || goto :err
"%LOCALAPPDATA%\NOVA\nova.exe" --autostart on
start "" "%LOCALAPPDATA%\NOVA\nova.exe" --tray
echo Selesai. Asisten aktif di system tray dan akan muncul otomatis saat laptop menyala.
pause
exit /b 0
:err
echo Gagal. Pastikan Rust terpasang ^(https://rustup.rs^).
pause
exit /b 1
