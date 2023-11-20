FROM ubuntu:22.04

RUN apt update

RUN DEBIAN_FRONTEND=noninteractive apt install -y \
        --allow-unauthenticated --allow-downgrades --allow-change-held-packages --no-install-recommends \
        sudo \
        build-essential \
        tzdata \
        ca-certificates \
        openssh-server \
        curl \
        wget \
        bison \
        iproute2 \
        iputils-ping dnsutils pciutils \
        iftop \
        unzip \
        cmake \
        git \
        lsb-release \
        numactl \
        pkg-config \
        libssl-dev \
        telnet \
        vim

RUN mkdir -p /var/run/sshd
RUN passwd -d root
RUN sed -i'' -e's/^#PermitRootLogin prohibit-password$/PermitRootLogin yes/' /etc/ssh/sshd_config \
        && sed -i'' -e's/^#PasswordAuthentication no$/PasswordAuthentication yes/' /etc/ssh/sshd_config \
        && sed -i'' -e's/^#PermitEmptyPasswords no$/PermitEmptyPasswords yes/' /etc/ssh/sshd_config \
        && sed -i'' -e's/^UsePAM yes/UsePAM no/' /etc/ssh/sshd_config

RUN DEBIAN_FRONTEND=noninteractive apt install -y \
        --allow-unauthenticated --allow-downgrades --allow-change-held-packages --no-install-recommends \
        libclang-dev libnuma-dev librdmacm-dev libibverbs-dev protobuf-compiler

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install cargo-make

RUN cd /root && git clone https://github.com/kristoff-starling/phoenix.git -b rpc_echo --recursive

WORKDIR /root/phoenix

RUN cargo make build-phoenixos
RUN cargo make build-plugins
RUN cargo make deploy-plugins
RUN cargo make build-phoenix-cli
RUN cargo make --cwd experimental/mrpc build-mrpc-plugins
RUN cargo make --cwd experimental/mrpc deploy-mrpc-plugins
RUN cargo make --cwd experimental/mrpc build-rpc-echo
RUN find experimental/mrpc/target/release -maxdepth 1 -name "rpc_echo_*" | xargs cp -t target/phoenix/release