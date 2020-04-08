version = $(shell awk '/^version/' Cargo.toml | head -n1 | cut -d "=" -f 2 | sed 's: ::g')
release := "1"
uniq := $(shell head -c1000 /dev/urandom | sha512sum | head -c 12 ; echo ;)
cidfile := "/tmp/.tmp.docker.$(uniq)"
build_type := release

all:
	mkdir -p build/ && \
	cp Dockerfile.build.ubuntu18.04 build/Dockerfile && \
	cp -a Cargo.toml src aws_app_lib aws_app_http scripts \
		templates Makefile build/ && \
	cd build/ && \
	docker build -t aws_app_rust/build_rust:ubuntu18.04 . && \
	cd ../ && \
	rm -rf build/

cleanup:
	docker rmi `docker images | python -c "import sys; print('\n'.join(l.split()[2] for l in sys.stdin if '<none>' in l))"`
	rm -rf /tmp/.tmp.docker.aws_app_rust
	rm Dockerfile

package:
	docker run --cidfile $(cidfile) -v `pwd`/target:/aws_app_rust/target aws_app_rust/build_rust:ubuntu18.04 \
        /aws_app_rust/scripts/build_deb_docker.sh $(version) $(release)
	docker cp `cat $(cidfile)`:/aws_app_rust/aws-app-rust_$(version)-$(release)_amd64.deb .
	docker rm `cat $(cidfile)`
	rm $(cidfile)

test:
	docker run --cidfile $(cidfile) -v `pwd`/target:/aws_app_rust/target aws_app_rust/build_rust:ubuntu18.04 /bin/bash -c ". ~/.cargo/env && cargo test"

build_test:
	cp Dockerfile.test.ubuntu18.04 build/Dockerfile && \
	cd build/ && \
	docker build -t aws_app_rust/test_rust:ubuntu18.04 . && \
	cd ../ && \
	rm -rf build/

install:
	cp target/$(build_type)/aws-app-rust /usr/bin/aws-app-rust
	cp target/$(build_type)/aws-app-http /usr/bin/aws-app-http

pull:
	`aws ecr --region us-east-1 get-login --no-include-email`
	docker pull 281914939654.dkr.ecr.us-east-1.amazonaws.com/rust_stable:latest
	docker tag 281914939654.dkr.ecr.us-east-1.amazonaws.com/rust_stable:latest rust_stable:latest
	docker rmi 281914939654.dkr.ecr.us-east-1.amazonaws.com/rust_stable:latest

dev:
	docker run -it --rm -v `pwd`:/aws_app_rust rust_stable:latest /bin/bash || true

get_version:
	echo $(version)

profile:
	sudo su -c "echo -1 > /proc/sys/kernel/perf_event_paranoid"
	perf record --call-graph dwarf ./target/debug/aws-app-rust list -r reserved spot ami volume snapshot ecr key
	perf script | inferno-collapse-perf > stacks.folded
	cat stacks.folded | inferno-flamegraph > flamegraph.svg
