# Rust Driver Samples

This is a Rust port of the driver samples from the original [Windows Driver Samples on Github.](https://github.com/microsoft/Windows-driver-samples)

The repository provides examples and best practices for Windows driver development in Rust using crates from [windows-drivers-rs.](https://github.com/microsoft/windows-drivers-rs)

## Getting Started

### Pre-requisites

#### Required

* Set up [EWDK Build Environment](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk)
  * Easy install option:
    * Install the latest version from link
      * <https://learn.microsoft.com/en-us/legal/windows/hardware/enterprise-wdk-license-2022>
    * Expand the ISO image to c:\ewdk
    * Start Environment by running in command prompt:
      * ```c:\ewdk\LaunchBuildEnv.cmd```
* Install [Clang](https://clang.llvm.org/get_started.html)
  * Easy install option:
    * `winget install LLVM.LLVM`

* Install [Rust](https://www.rust-lang.org/tools/install)
  * Easy install option for x64 systems:

```pwsh
Invoke-RestMethod -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile "$env:USERPROFILE\Downloads\rustup-init.exe"
& "$env:USERPROFILE\Downloads\rustup-init.exe" -y
```

#### Rust Setup

Run the following commands after setting up Rust.

`cargo install cargo-make --no-default-features --features tls-native`

__Note on arm64: ARM64 support for ring is [not released yet](https://github.com/briansmith/ring/issues/1167), so TLS features must be disabled until arm64 is officially supported by ring (probably in 0.17.0 release)__

##### Optional

These are not-required, but may make it easier to work in a rust environment:

`cargo install cargo-expand cargo-edit cargo-workspaces`

## Documentation

`cargo doc --document-private-items --open`

## Build and Test

### Build

From an EWDK development command prompt, run:

`cargo make`

If build is successful, this will stamp the INF and create a CAT file placed with driver binary and INF in `Package` folder.

### Install

#### One Time PC Setup

1. If Bitlocker is enabled, suspend Bitlocker
    Example: `manage-bde -protectors -disable C:`
1. Turn off Secure Boot via your UEFI/BIOS Settings
    Example: `shutdown -r -o -t 0` then pick Advanced -> UEFI Settings
1. If Bitlocker is enabled, suspend Bitlocker again
    Example: `manage-bde -protectors -disable C:`
1. Turn on test signing
    `bcdedit /set testsigning on`
1. Reboot
    `shutdown -r -t 0`

1. Copy the following to the DUT (Device Under Test: the computer you want to test the driver on):
   1. `.\target\..configuration..\package`
   1. `Path\To\Driver\DriverCertificate.cer`
   1. The version of `devgen.exe` from the WDK Developer Tools that matches the archtecture of your DUT
     * Ex. `C:\ewdk\Program Files\Windows Kits\10\Tools\10.0.22621.0\x64\devgen.exe`
1. Install the Certificate on the DUT:
   1. Double click the certificate
   1. Click Install Certificate
   1. Store Location: Local Machine -> Next
   1. Place all certificates in the following Store -> Browse -> Trusted Root Certification Authorities -> Ok -> Next
   1. Finish
1. Install the driver from Admin Command Prompt:
   1. In the package directory, run: `pnputil.exe /add-driver echo_2.inf /install`
1. Create a software device from Admin Command Prompt:
   1. In the directory that `devgen.exe` was copied to, run: `devgen.exe /add /hardwareid "root\ECHO_2"`

### Test

* To capture prints:
  * Start [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
    1. Enable `Capture Kernel`
    2. Enable `Enable Verbose Kernel Output`
  * Alternatively, you can see prints in an active Windbg session.
    1. Attach WinDBG
    2. `ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF`

### Usage

The echo driver can be tested by using the [echo executable](https://github.com/microsoft/Windows-driver-samples/tree/main/general/echo/kmdf/exe)

* Echoapp.exe --- Send single write and read request synchronously

* Echoapp.exe -Async --- Send 100 reads and writes asynchronously

Exit the app anytime by pressing Ctrl-C

## Windows driver development

### Windows Driver Kit (WDK)

Take a look at the compilation of the new and changed driver-related content for Windows 11. Areas of improvement include camera, print, display, Near Field Communication (NFC), WLAN, Bluetooth, and more.

[Find out what's new in the WDK](https://docs.microsoft.com/windows-hardware/drivers/what-s-new-in-driver-development)

### Windows Driver Frameworks

The Windows Driver Frameworks (WDF) are a set of libraries that make it simple to write high-quality device drivers.

[WDF driver development guide](https://docs.microsoft.com/windows-hardware/drivers/wdf/)

### Samples

Use the samples in this repo to guide your Windows driver development. Whether you're just getting started or porting an older driver to the newest version of Windows, code samples are valuable guides on how to write drivers.

For information about important changes that need to be made to the WDK sample drivers before releasing device drivers based on the sample code, see the following topic:

[From Sample Code to Production Driver - What to Change in the Samples](https://docs.microsoft.com/en-us/windows-hardware/drivers/gettingstarted/from-sample-code-to-production-driver)

## Trademarks

This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft
trademarks or logos is subject to and must follow
[Microsoft's Trademark & Brand Guidelines](https://www.microsoft.com/en-us/legal/intellectualproperty/trademarks/usage/general).
Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship.
Any use of third-party trademarks or logos are subject to those third-party's policies.
