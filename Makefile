.PHONY: all build clean run

build:
	docker build -t brickit .

eif:
	./scripts/dexport.sh brickit

clean:
	cargo clean
	cd enclave && cargo clean && cd ..
	rm -f nitro.eif nitro.pcrs

run:
	cargo run --release -- noboot

send:
	cargo run -p sender -- signer-app.bin
