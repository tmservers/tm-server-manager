#===========================================================
# Dockerfile which patches the musl target
# with mimalloc to enable better performance.
# Based on this post: https://users.rust-lang.org/t/static-linking-for-rust-without-glibc-scratch-image/112279/5
# Inlined the build images into this one file which will
# increase build times but i like the simplicity and we wnot
# do many builds/releases either way so it should be fine :>
#===========================================================

FROM rust:1.93.1-alpine3.23 AS builder

# Update apk packagte list
RUN apk upgrade --no-cache

# Add build dependencies
RUN apk add --no-cache alpine-sdk cmake mold samurai make protoc protobuf-dev musl-dev curl perl

# Add musl build target for Intel/AMD linux
RUN rustup target add x86_64-unknown-linux-musl

# Add script and patch
COPY docker/build.sh docker/mimalloc.diff /tmp/

# Make script executable
RUN chmod +x /tmp/build.sh

# Run script to build mimalloc and patch default allocator to use mimalloc
RUN /tmp/build.sh

# Export environment variable to override default memory allocator
ENV LD_PRELOAD=/usr/lib/libmimalloc.so

# Set workspace directory
WORKDIR /app

# Copy over the entire source code
COPY . ./

# Fetch dependencies if they change.
RUN set -x && cargo fetch

# Run the release build for package cmdb.
RUN cargo build -p tm-server-bridge --release --features="docker" --target x86_64-unknown-linux-musl

# Move binary up to root level directory for easy access
RUN mv /app/target/x86_64-unknown-linux-musl/release/tm-server-bridge /tm-server-bridge

# Strip the binary to reduce its size
RUN strip -s /tm-server-bridge


FROM rust:1.93.1-alpine3.23 AS setup

# Create the user and group files to run the binary as an unprivileged user.
RUN mkdir /user && \
    echo 'nobody:x:65534:65534:nobody:/:' > /user/passwd && \
    echo 'nobody:x:65534:' > /user/group

############################
# Scratch image
############################
# Create minimal docker image
FROM scratch

# Import user and group files from the build stage.
COPY --from=setup /user/group /user/passwd /etc/

# Import the CAcertificates from the build stage to enable HTTPS.
COPY --from=setup /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

COPY --from=builder /tm-server-bridge /

USER nobody:nobody

CMD ["/tm-server-bridge"]