FROM stagex/eif_build:0.2.2@sha256:291653f1ca528af48fd05858749c443300f6b24d2ffefa7f5a3a06c27c774566 AS eif_build
FROM stagex/gen_initramfs:6.8@sha256:f5b9271cca6003e952cbbb9ef041ffa92ba328894f563d1d77942e6b5cdeac1a AS gen_initramfs
FROM stagex/iproute2:sx2024.11.0@sha256:65da03aa94d17dd6310b022f426a6cc8b3c55bb267e4bac1697bc57d6c850570 AS iproute2
FROM stagex/libcap:sx2024.11.0@sha256:7fbaa6bae0f944a3916eccfb978ff758ed2bad56ef3b4a86d39454f3587b38d2 AS libcap
FROM stagex/iputils:sx2024.11.0@sha256:979fdeb70e03c7305aff526f16b6f9f0e503862337ae61f576e17b3d48aad8f6 AS iputils
FROM stagex/musl:sx2024.11.0@sha256:d7f6c365f5724c65cadb2b96d9f594e46132ceb366174c89dbf7554897f2bc53 AS musl
# NB(scm): reverted to the old linux-nitro on the recommendation from Lance:
#  the latest linux kernel crashes the nitro enclave.
#FROM stagex/linux-nitro:5.19.6@sha256:e6c8a861f9b18edfad56b1aa130feb822a25987c71e2b2932b020750dd7325bc AS linux-nitro
FROM stagex/linux-nitro:sx2024.03.0@sha256:073c4603686e3bdc0ed6755fee3203f6f6f1512e0ded09eaea8866b002b04264 AS linux-nitro

FROM ghcr.io/tkhq/base/rust:sha-2f7790d638553221661f477c8c61abef36af00d4@sha256:f35ee463ce91ac8108e5fc2b400a7ca36ff9ecffffd7a8ed02f63a8cdd9344d9 AS build
ADD . /src/

ENV CARGOFLAGS='--target x86_64-unknown-linux-musl --locked --release'
ENV CARGO_HOME=/tmp/rust
ENV RUSTFLAGS='-C target-feature=+crt-static'

FROM build AS build-init
WORKDIR /src
RUN cargo build ${CARGOFLAGS}
RUN cp target/x86_64-unknown-linux-musl/release/init /
RUN file /init | grep "static-pie"

FROM build AS build-eif
WORKDIR /build_cpio
COPY --from=eif_build . /
COPY --from=gen_initramfs . /
COPY --from=build-init /init .
COPY --from=linux-nitro /nsm.ko .
COPY --from=iproute2 . /
COPY --from=libcap . /
COPY --from=iputils . /
COPY --from=musl . /
COPY out/hosts.file /hosts.file
COPY out/downer /downer
COPY out/sender /sender
COPY <<-EOF initramfs.list
	file /init     init    0700 0 0
	file /nsm.ko   nsm.ko  0600 0 0
	dir  /run              0755 0 0
	dir  /tmp              0755 0 0
	dir  /etc              0755 0 0
	dir  /bin              0755 0 0
	dir  /sbin             0755 0 0
	dir  /proc             0755 0 0
	dir  /sys              0755 0 0
	dir  /usr              0755 0 0
	dir  /lib              0755 0 0
	dir  /usr/bin          0755 0 0
	dir  /usr/sbin         0755 0 0
	dir  /usr/lib          0755 0 0
	dir  /dev              0755 0 0
	dir  /dev/shm          0755 0 0
	dir  /dev/pts          0755 0 0
	file /usr/sbin/ip      /usr/sbin/ip     0700 0 0
	file /usr/bin/ldd      /usr/bin/ldd     0700 0 0
	file /usr/lib/libcap.so.2    /usr/lib/libcap.so.2    0644 0 0
  file /usr/lib/libcap.so.2.70 /usr/lib/libcap.so.2.70 0644 0 0
	file /usr/bin/ping     /usr/bin/ping    0700 0 0
	file /lib/ld-musl-x86  /usr/lib/ld-musl-x86_64.so.1                   0700 0 0
	file /usr/lib/libc.musl-x86_64.so.1 /usr/lib/libc.musl-x86_64.so.1    0644 0 0
	file /downer    /downer   0700 0 0
	file /sender    /sender   0700 0 0
	file /hosts.file       /etc/hosts       0644 0 0
	nod  /dev/console      0600 0 0 c 5 1
EOF
ENV CPIO_TIMESTAMP=1
ENV KBUILD_BUILD_TIMESTAMP=1
RUN <<-EOF
	find . -exec touch -hcd "@0" "{}" +
    mkdir /build_eif
	gen_init_cpio -t 1 initramfs.list > /build_eif/rootfs.cpio
	touch -hcd "@0" rootfs.cpio
EOF
WORKDIR /build_eif
COPY --from=linux-nitro /bzImage .
COPY --from=linux-nitro /linux.config .
RUN eif_build \
	--ramdisk rootfs.cpio \
	--kernel bzImage \
	--kernel_config linux.config \
	--pcrs_output /nitro.pcrs \
	--output /nitro.eif \
	--cmdline 'reboot=k initrd=0x2000000,3228672 root=/dev/ram0 panic=1 pci=off nomodules console=ttyS0 i8042.noaux i8042.nomux i8042.nopnp i8042.dumbkbd'

# Starting "FROM scratch" is important here given this interacts with the nitro enclave to boot it
# No shell, no access to "core", just the bare minimum.
FROM scratch AS package
COPY --from=build-eif /nitro.eif .
# COPY --from=build-eif /nitro.pcrs .
ENTRYPOINT ["/bin/bash"]
