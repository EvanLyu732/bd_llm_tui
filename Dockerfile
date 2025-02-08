# 构建阶段
FROM rust:1.70 as builder

WORKDIR /usr/src/bd-llm-tui
COPY . .

# 安装构建依赖
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    cargo build --release

# 运行阶段
FROM debian:bullseye-slim

# 安装运行时依赖
RUN apt-get update && \
    apt-get install -y libssl1.1 && \
    rm -rf /var/lib/apt/lists/*

# 复制构建产物
COPY --from=builder /usr/src/bd-llm-tui/target/release/llm_tui /usr/local/bin/bd-llm-tui

# 设置工作目录
WORKDIR /root

# 创建配置目录
RUN mkdir -p /root/.config/bd-llm-tui

# 设置环境变量
ENV TERM=xterm-256color

# 入口命令
ENTRYPOINT ["bd-llm-tui"] 