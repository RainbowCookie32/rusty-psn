FROM amd64/ubuntu:impish
WORKDIR /rusty-psn

RUN apt-get update && \
    apt-get install -y curl openssl unzip

RUN curl -sL https://github.com/RainbowCookie32/rusty-psn/releases/latest/download/rusty-psn-cli-linux.zip -o rusty-psn-cli-linux.zip && \
    unzip rusty-psn-cli-linux.zip && \
    rm rusty-psn-cli-linux.zip && \
    chmod +x rusty-psn

ENTRYPOINT [ "/rusty-psn/rusty-psn" ]
