@echo off
rem Mematikan nyala otomatis NOVA dan menghentikan prosesnya. Data (assistant.db dll.) tidak dihapus.
if exist "%LOCALAPPDATA%\NOVA\nova.exe" "%LOCALAPPDATA%\NOVA\nova.exe" --autostart off
taskkill /f /im nova.exe >nul 2>&1
echo Asisten dimatikan dari startup.
pause
