.PHONY: clean run send

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
	cargo run -p sender -- signer-app.bin
