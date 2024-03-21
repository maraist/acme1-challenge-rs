# Recall AI 

## Transcode + Job-Managing web-server

What follows is a coding-challenge submission

It contains two parts (representing the two stages of the challenge)

The recallai is the code-functionality

The web-server is the hosted solution including a job-management system

# BUILD

## warnings

A release build will build for native; if this isn't what you want, comment out the recallai/Cargo.toml C-flag (last line)

## Local

### Local web-server 

    cargo build --release

### Library binaries

    cd recallai
    cargo build --release

# Docker containers

## Podman build (redhat systems)

    podman build --tag acme:recallai -f Dockerfile
    podman run -d -p 8080:8080 acme:recallai 

## Docker build

    docker build --tag acme:recallai -f Dockerfile
    docker run -d -p 8080:8080 acme:recallai

## docker-resource

The challenge asked for a debian-based install; this wound up not supporting the io-uring in time (not sure if debian uses a new
enough kernel).  Ideally I would have used an amazon 2023 base image or a rust-base-image

# Evaluation scripts

Both the recallai/scripts and web-server/scripts folders contain various tools to pre-process the data or evaluate the
coding challenge results
