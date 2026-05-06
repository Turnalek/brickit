.PHONY: clean run send vhost kill qemu

vhost:
	RUST_LOG=debug vhost-device-vsock --vm guest-cid=4,forward-cid=1,forward-listen=9001,socket=/tmp/vhost4.socket

stop:
	-killall qemu-system-x86_64
	# -killall vhost-device-vsock

qemu: out/nitro.eif
	qemu-system-x86_64 -M nitro-enclave,vsock=c,id=hello-world -kernel out/nitro.eif -nographic -m 4G --enable-kvm -cpu host -chardev socket,id=c,path=/tmp/vhost4.socket

out/nitro.tar: Dockerfile init/Cargo.toml init/src/*.rs common/Cargo.toml common/src/*.rs
	docker build -t brickit -f Dockerfile . --output type=tar,dest=out/nitro.tar

out/sender.tar: Dockerfile.sender sender/Cargo.toml sender/src/main.rs common/Cargo.toml common/src/*.rs
	docker build -t sender -f Dockerfile.sender . --output type=tar,dest=out/sender.tar

out/nitro.eif: out/nitro.tar
	tar -xf out/nitro.tar -C out

out/sender: out/sender.tar
	tar -xf out/sender.tar -C out

clean:
	cargo clean
	rm -f out/*

run:
	cargo run --release -- noboot

send:
	cargo run -p sender -- README.md
