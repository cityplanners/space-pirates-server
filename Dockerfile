FROM rust:1.67

WORKDIR /space-pirates-server

RUN apt update && apt-get -y install protobuf-compiler

COPY . .

RUN cargo install --path .

EXPOSE 50051

CMD ["space-pirates-server"]