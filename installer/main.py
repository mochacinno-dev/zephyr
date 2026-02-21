# ════════════════════════════════════════════════════════════
# Zephyr Installer Utility
# ════════════════════════════════════════════════════════════

# ── Libraries ───────────────────────────────────────────────
import os
import platform
from time import sleep

# ── Globals ─────────────────────────────────────────────────
ver = "0.9.8"

def clear():
    """
    Clears the terminal screen based on the operating system.
    """
    # Check the operating system
    if platform.system().lower() == "windows":
        # Command for Windows
        os.system('cls')
    else:
        # Command for Linux and macOS (POSIX systems)
        os.system('clear')

def install():
    """
    Handles the installation process.
    """
    path = "C:\\Program Files\\Zephyr"

    # ── Creation ────────────────────────────────────────────
    try:
        os.makedirs(path, exist_ok=True)
        print(f"Folder '{path}' created or already exists.")
    except OSError as e:
        # Handle other potential errors, such as permission issues
        print(f"Error creating folder: {e}")

    # ── Act. Instal ─────────────────────────────────────────
    full_path = os.path.join(path, "test.txt")
    with open(full_path, 'w') as file:
        file.write("THIS IS A TEST FOR THE ZEPHINST PROGRAM.")

def main():
    print("VERSION 0.0.1 - ZEPHINST")
    print("Zephyr Installer Utility for Windows.")
    
    print(f"Do you want to install Zephyr {ver}. [Y/N]")
    ic = input("> ")
    capic = ic.upper()

    if capic == "Y":
        install()
    elif capic == "N":
        clear()
        print("Thank you for using the installer.")
    else:
        clear()
        print(f"INVALID INPUT ({ic} / {capic}) - ERROR 00.1")
        sleep(5.0)
        clear()
        main()

if __name__ == "__main__":
    main()