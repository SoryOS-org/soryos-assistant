#!/usr/bin/env python3
"""
Build script for wake word detection executable
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import PyInstaller.__main__

BASE_DIR = Path(__file__).resolve().parent
SIDECAR_BASENAME = "sprite_voice_sidecar"
ICON_PATH = BASE_DIR.parent / "tauri" / "src-tauri" / "icons" / "icon.ico"

def build_executable():
    """Build standalone executable using PyInstaller"""

    os.chdir(BASE_DIR)
    
    print("Building wake word detection executable...")
    
    # Clean previous builds
    if os.path.exists("dist"):
        shutil.rmtree("dist")
    if os.path.exists("build"):
        shutil.rmtree("build")
    
    args = [
        str(BASE_DIR / 'voice_processor.py'),
        '--onefile',
        f'--name={SIDECAR_BASENAME}',
        '--distpath=dist',
        '--workpath=build',
        '--specpath=build',
        '--clean',
        '--noconfirm',
        '--collect-all=pvporcupine',
        '--hidden-import=pyaudio',
    ]
    if ICON_PATH.exists():
        args.extend(['--icon', str(ICON_PATH)])
    
    try:
        # Run PyInstaller
        PyInstaller.__main__.run(args)

        print(f"Executable built successfully in dist/{SIDECAR_BASENAME}.exe")

        # Check if executable was created
        exe_path = Path("dist") / f"{SIDECAR_BASENAME}.exe"
        if exe_path.exists():
            size = exe_path.stat().st_size
            print(f"Executable size: {size / (1024*1024):.1f} MB")
            _copy_into_tauri_resources(exe_path)
        else:
            print("ERROR: Executable not found")
            return False
        
        return True
        
    except Exception as e:
        print(f"ERROR: Build failed: {e}")
        return False


def _copy_into_tauri_resources(executable_path: Path) -> None:
    """Copy the freshly built executable into the Tauri resources folder."""
    _try_stop_running_processes()

    tauri_resources = BASE_DIR.parent / "tauri" / "src-tauri" / "resources"
    target_path = tauri_resources / executable_path.name

    try:
        tauri_resources.mkdir(parents=True, exist_ok=True)
        shutil.copy2(executable_path, target_path)
        print(f"Copied executable to {target_path}")
    except Exception as exc:
        print(f"WARNING: failed to copy executable to {target_path}: {exc}")


def _try_stop_running_processes() -> None:
    """Attempt to stop any running sidecar executables on Windows."""
    if os.name != "nt":
        return

    try:
        subprocess.run(
            ["taskkill", "/IM", f"{SIDECAR_BASENAME}.exe", "/F"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except Exception as exc:
        print(f"WARNING: unable to terminate {SIDECAR_BASENAME}.exe automatically: {exc}")


if __name__ == "__main__":
    print("Wake Word Detection Build Script")
    print("=" * 35)
    
    # Check if PyInstaller is available
    try:
        import PyInstaller
        print("PyInstaller available")
    except ImportError:
        print("ERROR: PyInstaller not found. Install with: pip install pyinstaller")
        sys.exit(1)
    
    # Build executable
    if build_executable():
        print("\nBuild completed successfully.")
        print(f"\nOutput: dist/{SIDECAR_BASENAME}.exe")
        print("\nTo use with Rust app, update stt.config.json:")
        print(f'  "executable_path": "path/to/dist/{SIDECAR_BASENAME}.exe"')
    else:
        print("\nBuild failed.")
        sys.exit(1)
