FROM debian:bookworm-slim

# Common deps

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl            \
    git             \
    ripgrep         \
    build-essential \
    python3         \
    && rm -rf /var/lib/apt/lists/*

# Rust

ENV PATH="/root/.cargo/bin:${PATH}"
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y -c rust-analyzer
RUN rustc --version && rust-analyzer --version

# Opencode

ENV OPENCODE_VERSION="v1.18.32"
ENV OPENCODE_URL="https://github.com/anomalyco/opencode/releases/download/${OPENCODE_VERSION}/opencode-linux-x64.tar.gz"

RUN curl -fsSL -o /tmp/opencode.tar.gz "${OPENCODE_URL}"
RUN tar -xf /tmp/opencode.tar.gz -C /usr/local/bin opencode
RUN rm /tmp/opencode.tar.gz
RUN chmod +x /usr/local/bin/opencode && opencode --version

# Use non-root user even inside the container

RUN useradd -m -u 1000 agent
USER agent

WORKDIR /work
ENTRYPOINT ["opencode"]
