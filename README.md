# Talos/AWS nitro vsock recv/send bug example

This is an example use case to reproduce a recv/send issue with vsock running on AWS nitro enclaves with Talos OS
verison v1.12.6.

This example code works without issue on version v1.12.5 and older.

This example sends a bytes payload from a file into the vsock on the host side and received it in the nitro enclave.
Both sides log syscall uses of recv/send calls and calculate sha256 sum of the data.

The data is sent with a separate header send of 8 bytes u64 represented in little endiant to indicated expected size.

The bug surfaces if the data being sent (without the header added) is over 32768 bytes long.

## Building the example binaries

The binaries are `nitro.eif` for the enclave kernel + runtime/receiver and `sender` binary to be run on the host side.
Requirements are:
1. docker with buildx
2. tar

Use `make out/nitro.eif` and `make out/sender` to build them.

## Deployment

An AWS ec2 instance compatible with Talos v1.12.5 needs to be created. We used the AMD instance type, but both should work.
The talos single node cluster needs to be initiated and a debug pod needs to be created in the deployment.

Connect to the debug pod and install the required packages with dnf.

TODO: add concrete setup steps

## Running the binaries

You can generate the random data payloads with
```bash
dd if=/dev/random of=data.good bs=32768 count=1
dd if=/dev/random of=data.bad bs=32769 count=1
```

Run the nitro.eif file from the debug pod by issuing:
```bash
nitro-cli run-enclave -│send 32769 bytes of payload
-cpu-count 2 --memory 512 --eif-path /root/nitro.e│done sending, total sent bytes 32769
if --enclave-cid 16 --debug-mode --attach-console
```

In a separate terminal, on the same debug pod (tmux etc.) run the sender binary such as

```bash
./sender data.good
./sender data.bad
```
