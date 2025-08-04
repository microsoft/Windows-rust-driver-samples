# Fail_Driver_Pool_Leak Sample Driver

The `fail_driver_pool_leak` sample demonstrates running the [Device Fundamentals Tests](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/device-fundamentals-tests) and enabling the [Driver Verifier](https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/driver-verifier) for a Rust driver. We have intentionally injected a pool leak fault in the driver by allocating a global buffer using WDM's `ExAllocatePool2` function and not freeing this buffer (using `ExFreePool`) anywhere in the driver. This fault, which is not caught at compile time, can be detected by running the Device Fundamentals Tests and also by enabling the Driver Verifier on the driver.

## Steps

1. Clone the repository and navigate to the project root.

2. Install [Clang](https://clang.llvm.org/get_started.html)
    * Easy install option:
    ```
    winget install LLVM.LLVM
    ```

3. Build the driver project using the following command in an [EWDK environment](https://learn.microsoft.com/en-us/legal/windows/hardware/enterprise-wdk-license-2022) - 
    ```
    cargo make
    ```
4. Prepare a target system (a Hyper-V VM can be used) for testing

    Follow the below steps to setup the test system -
    1. Disable Secure boot and start the system
    2. Run "ipconfig" on the host system and note down the IP (if you are using Default Switch for the VM, note down the IP on the Default Switch)
    3. Install and open WinDbg, click on "Attach to Kernel". The key for the connection will be generated in the test system in the next steps. 
    4. Connect to the test VM and run the following commands - 
        ```
        bcdedit /set testsigning on
        bcdedit /debug on
        bcdedit /dbgsettings net hostip:<PASTE.HOST.IP.HERE> port:<50000-50030>

        ### Copy the key string output by the above command
        ```
    5. Paste the key in host's WinDbg prompt and connect to the kernel
    6. Restart the target/test system 
        ```
        shutdown -r -t 0
        ```

5. Copy the driver package, available under ".\target\debug\fail_driver_pool_leak_package" to the target system.

6. Copy "devgen.exe" from host to the target system. Alternatively you may install WDK on the target system and add the directory that contains "devgen.exe" to PATH variable.

7. Install the driver package and create the device in the target system using the below commands - 
    ```
    cd "fail_driver_pool_leak_package"
    devgen.exe /add /bus ROOT /hardwareid "fail_driver_pool_leak"

    ## Copy the Device ID. This will be used later to run the tests

    pnputil.exe /add-driver .\fail_driver_pool_leak.inf /install
    ```
8. Enable Driver Verifier for 'fail_driver_pool_leak.sys' driver package 
    1. Open run command prompt (Start + R) or cmd as administator and run "verifier"
    2. In the verifier manager,
        - Create Standard Settings
        - Select driver names from list
        - Select 'fail_driver_pool_leak.sys'
        - Finish
        - Restart the system

9. Follow the steps in https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/how-to-test-a-driver-at-runtime-from-a-command-prompt to run tests against the device managed by this driver

10. Install TAEF and WDTF on the test computer and run the following test -
    ```
    cd "C:\Program Files (x86)\Windows Kits\10\Testing\Tests\Additional Tests\x64\DevFund"
    TE.exe .\Devfund_PnPDTest_WLK_Certification.dll /P:"DQ=DeviceID='ROOT\DEVGEN\{PASTE-DEVICE-ID-HERE}'" --rebootResumeOption:Manual
    ```

11. The test will lead to a Bugcheck and a BlueScreen on the target system with the following error - 
    ```
    DRIVER_VERIFIER_DETECTED_VIOLATION (c4)
    ```    
    Run ```!analyze -v``` for detailed bugcheck report
    
    Run ```!verifier 3 fail_driver_pool_leak.sys``` for info on the allocations that were leaked that caused the bugcheck.

12. (Alternatively), the bugcheck can be observed when all the devices managed by this driver are removed, i.e, when the driver is unloaded from the system. 
    You may use pnputil/devcon to enumerate and remove the devices -
    ```
    # To enumerate the devices
    pnputil /enum-devices 
    # To remove a device
    pnputil /remove-device "DEVICE-ID"
    ```

### References

- [TAEF](https://learn.microsoft.com/en-us/windows-hardware/drivers/taef/getting-started)
- [WDTF](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdtf/wdtf-runtime-library)
- [Testing a driver at runtime](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/how-to-test-a-driver-at-runtime-from-a-command-prompt)
- [Using WDF to Develop a Driver](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/using-the-framework-to-develop-a-driver)
- [wdfmemory](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfmemory/)