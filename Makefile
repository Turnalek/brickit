.PHONY: clean run send vhost kill qemu downer upload stop ssh

EC2 = ec2-18-119-111-214.us-east-2.compute.amazonaws.com

vhost:
	RUST_LOG=debug vhost-device-vsock --vm guest-cid=4,forward-cid=1,forward-listen=9001,socket=/tmp/vhost4.socket

stop:
	-killall qemu-system-x86_64
	# -killall vhost-device-vsock

qemu: out/nitro.eif
	qemu-system-x86_64 -M nitro-enclave,vsock=c,id=hello-world -kernel out/nitro.eif -nographic -m 4G --enable-kvm -cpu host -chardev socket,id=c,path=/tmp/vhost4.socket

out/nitro.eif: Dockerfile init/Cargo.toml init/src/*.rs common/Cargo.toml common/src/*.rs downer/src/*.rs downer/Cargo.toml
	docker build -t brickit -f Dockerfile . --output type=tar,dest=out/nitro.tar
	tar -xf out/nitro.tar -C out

out/sender.tar: Dockerfile.sender sender/Cargo.toml sender/src/main.rs common/Cargo.toml common/src/*.rs
	docker build -t sender -f Dockerfile.sender . --output type=tar,dest=out/sender.tar

out/sender: out/sender.tar
	tar -xf out/sender.tar -C out

clean:
	cargo clean
	rm -f out/* downer.x86_64

run:
	cargo run --release -- noboot

host:
	cargo run --release -p sender

target/x86_64-unknown-linux-musl/release/sender: sender/src/main.rs sender/Cargo.toml
	cargo build --release --target x86_64-unknown-linux-musl -p sender

downer:
	cargo build --release --target x86_64-unknown-linux-musl -p downer
	cp target/x86_64-unknown-linux-musl/release/downer downer.x86_64

upload: out/nitro.eif target/x86_64-unknown-linux-musl/release/sender
	scp -i ~/.ssh/TURNKEY_TALOS_TEST.pem enclave_egress_interfaces.sh \
	out/nitro.eif \
	target/x86_64-unknown-linux-musl/release/sender \
	ec2-user@$(EC2):~

ssh:
	ssh -i ~/.ssh/TURNKEY_TALOS_TEST.pem ec2-user@$(EC2)
