@echo off
chcp 65001 >nul
echo ==================================================
echo   MY Screen 7.1 Professional - Rust Derleyici
echo ==================================================
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo [HATA] Rust (cargo) kurulu degil!
    echo Lutfen https://rustup.rs adresinden Rust'i kurun.
    pause
    exit /b 1
)
echo Derleme baslatiliyor (Release modu)...
cargo build --release
if %errorlevel% neq 0 (
    echo [HATA] Derleme basarisiz oldu.
    pause
    exit /b 1
)
copy /Y "target\release\my_screen_professional.exe" "MY Screen 7.1 Professional.exe" >nul
echo.
echo [BASARILI] "MY Screen 7.1 Professional.exe" olusturuldu!
pause
