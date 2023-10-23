FROM rust:alpine as build

WORKDIR /app

RUN apk update
RUN apk add git gcc musl-dev
COPY . /app/tuic

WORKDIR /app/tuic
RUN cargo build --release --config net.git-fetch-with-cli=true --package tuic-server
RUN cp ./target/release/tuic-server /usr/bin/tuic-server
RUN chmod +x /usr/bin/tuic-server
# END build

FROM alpine:latest as main

COPY --from=build /usr/bin/tuic-server /usr/bin/tuic-server

ENTRYPOINT [ "/usr/bin/tuic-server" ]
CMD [ "-c", "/etc/tuic/config.json" ]