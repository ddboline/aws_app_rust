FROM rust:alpine3.14 as buildstage

RUN apk add musl-dev && mkdir -p /build/aws_app_rust

ADD Cargo.toml /build/aws_app_rust/
COPY src /build/aws_app_rust/src
COPY aws_app_http /build/aws_app_rust/aws_app_http
COPY aws_app_lib /build/aws_app_rust/aws_app_lib
COPY migrations /build/aws_app_rust/migrations
COPY templates /build/aws_app_rust/templates

RUN mkdir -p /aws_app_rust && \
    cd /build/aws_app_rust && \
    cargo install --path=. --root=/aws_app_rust

FROM alpine:3.14

COPY --from=buildstage /aws_app_rust/bin/* /usr/bin/

STOPSIGNAL SIGTERM

EXPOSE 3097

CMD ["/usr/bin/aws-app-http"]
