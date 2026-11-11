#!/usr/bin/env python3
"""
Setup script for wake word detection service
"""

import subprocess
import sys
import os

def run_command(command, description):
    """Run a command and handle errors"""
    print(f"{description}...")
    try:
        result = subprocess.run(command, shell=True, check=True, capture_output=True, text=True)
        print(f"{description} completed")
        return True
    except subprocess.CalledProcessError as e:
        print(f"{description} failed: {e}")
        if e.stderr:
            print(f"Error: {e.stderr}")
        return False

def main():
    print("Setting up Wake Word Detection Service...")
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    
    # Check if Python is available
    if not run_command("python --version", "Checking Python version"):
        print("Python is not available. Please install Python 3.8+")
        sys.exit(1)
    
    # Install dependencies
    if not run_command("pip install -r requirements.txt", "Installing dependencies"):
        print("Failed to install dependencies")
        sys.exit(1)
    
    # Install PyInstaller for building
    if not run_command("pip install pyinstaller", "Installing PyInstaller"):
        print("Failed to install PyInstaller")
        sys.exit(1)
    
    print("Setup completed successfully!")


if __name__ == "__main__":
    main()
