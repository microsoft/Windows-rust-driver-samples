# Driver Verifier Pool Leak Sample

This KMDF sample contains an intentional error that demonstrates the capabilities and features of Driver Verifier and the Device Fundamentals tests.
    
The driver uses WDM's ExAllocatePool2 API to allocate memory in its Device Context buffer when a device is added by the PnP manager. However, this buffer is not freed anywhere in the driver, including the driver unload function.

By enabling Driver Verifier on this driver, the pool leak violation can be caught when the driver is unloaded and with an active KDNET session, the bug can be analyzed further.

## Steps to Reproduce the issue

1. Clone the repository and navigate to the project directory.

2. Build the driver project using the following command in a WDK environment (or EWDK prompt) - 
    ```
    cargo make
    ```
3. Prepare a target system (a Hyper-V VM can be used) for testing

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

4. Copy the driver package, available under ".\target\debug\pool_leak_package" to the target system.

5. Copy "devgen.exe" from host to the target system. Alternatively you may install WDK on the target system and add the directory that contains "devgen.exe" to PATH variable.

6. Install the driver package and create the device in the target system using the below commands - 
    ```
    cd "pool_leak_package"
    devgen.exe /add /bus ROOT /hardwareid "pool_leak"

    ## Copy the Device ID. This will be used later to run the tests

    pnputil.exe /add-driver .\pool_leak.inf /install
    ```
7. Enable Driver Verifier for 'pool_leak.sys' driver package 
    1. Open run command prompt (Start + R) or cmd as administator and run "verifier"
    2. In the verifier manager,
        - Create Standard Settings
        - Select driver names from list
        - Select 'pool_leak.sys'
        - Finish
        - Restart the system

8. Follow the steps in https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/how-to-test-a-driver-at-runtime-from-a-command-prompt to run tests against the device managed by this driver

9. Run the following test after TAEF and WDTF are installed -
    ```
    cd "C:\Program Files (x86)\Windows Kits\10\Testing\Tests\Additional Tests\x64\DevFund"
    TE.exe .\Devfund_PnPDTest_WLK_Certification.dll /P:"DQ=DeviceID='ROOT\DEVGEN\{PASTE-DEVICE-GUID-HERE}'" --rebootResumeOption:Manual
    ```

10. The test will lead to a Bugcheck and a BlueScreen on the target system with the following error - 
    ```
    DRIVER_VERIFIER_DETECTED_VIOLATION (c4)
    ```
    The logs will be available in WinDbg
    run ```!analyze -v``` for detailed bugcheck report
    run ```!verifier 3 pool_leak.sys``` for info on the allocations that were leaked that caused the bugcheck.

11. (Alternatively), the bugcheck can be observed when all the devices managed by this driver are removed. 
    You may use pnputil/devcon to enumerate and remove the devices -
    ```
    # To enumerate the devices
    pnputil /enum-devices 
    # To remove a device
    pnputil /remove-device "DEVICE/ID"
    ```

References

- https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/driver-verifier
- https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/device-fundamentals-tests
- https://learn.microsoft.com/en-us/windows-hardware/drivers/taef/getting-started
- https://learn.microsoft.com/en-us/windows-hardware/drivers/wdtf/wdtf-runtime-library
- https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/how-to-test-a-driver-at-runtime-from-a-command-prompt
