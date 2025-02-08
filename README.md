# BD-LLM-TUI

一个用于百度千帆大语言模型的终端用户界面（TUI）应用程序。

![bd_llm_tui demo](https://raw.githubusercontent.com/EvanLyu732/evanlyu732.github.io/refs/heads/main/static/images/tui.gif)

## 功能特点

- 清爽的终端界面
- 支持多种百度千帆模型
- Markdown 渲染支持
- 历史消息查看和滚动
- 快捷键操作
- 配置持久化
- 支持文本复制

## 支持的模型

- ERNIE 4.0 系列
- ERNIE 3.5 系列
- ERNIE Speed 系列
- ERNIE Lite 系列
- ERNIE Tiny
- ERNIE Character 系列
- DeepSeek 系列

## 安装

### 从 Debian 包安装

```bash
sudo dpkg -i bd-llm-tui_0.1.0_amd64.deb
```

### 从源码编译

需要 Rust 环境：

```bash
git clone https://gitlab.com/your-username/bd-llm-tui.git
cd bd-llm-tui
cargo build --release
```

### 使用 Docker

```bash
# 构建镜像
docker build -t bd-llm-tui .

# 运行容器
docker run -it --rm \
    -v $HOME/.config/bd-llm-tui:/root/.config/bd-llm-tui \
    bd-llm-tui
```

## 使用方法

1. 首次运行需要配置 API 认证令牌：
   - 按 Alt+C 打开配置界面
   - 输入您的百度千帆 API 令牌
   - 按 Enter 保存

2. 基本操作：
   - Enter: 发送消息
   - Tab: 切换输入框和历史框
   - ↑/↓: 滚动历史消息
   - Alt+H: 显示帮助
   - Alt+M: 切换模型
   - Alt+Y: 复制最后一条 AI 回复
   - Ctrl+C 或 Esc: 退出程序

## 快捷键列表

| 快捷键 | 功能 |
|--------|------|
| Alt+H | 显示帮助菜单 |
| Alt+C | 配置认证令牌 |
| Alt+M | 选择模型 |
| Alt+Y | 复制最后一条AI回复 |
| Tab | 切换输入框和历史框 |
| ↑/↓ | 在历史框中滚动 |
| Enter | 发送请求 |
| Ctrl+C | 退出程序 |
| Esc | 退出程序或关闭弹窗 |

## 配置文件

配置文件存储在：
- Linux: `~/.config/bd-llm-tui/config.json`

## 开发

### 依赖项

- Rust 1.70+
- Cargo
- 以下 Rust crates:
  - ratatui
  - crossterm
  - tokio
  - reqwest
  - serde
  - pulldown-cmark
  - 等

### 构建

```bash
cargo build --release
```

### 打包

```bash
# 创建 Debian 包
dpkg-buildpackage -us -uc
```

## 许可证

[MIT License](LICENSE)

## 贡献

欢迎提交 Issue 和 Pull Request！

## 注意事项

- 请妥善保管您的 API 认证令牌
- 确保有稳定的网络连接
- 留意 API 的使用配额

## 常见问题

1. Q: 如何获取百度千帆 API 令牌？
   A: 请访问[百度千帆官网](https://cloud.baidu.com/doc/WENXINWORKSHOP/s/Ilkkrb0i5)获取。

2. Q: 配置文件在哪里？
   A: Linux 系统下在 `~/.config/bd-llm-tui/config.json`。

3. Q: 如何切换模型？
   A: 按 Alt+M 打开模型选择界面，使用上下键选择，Enter 确认。

4. Q: 如何测试 API 是否正常？
   A: 可以使用项目中的 example.sh 脚本进行测试：
   ```bash
   ./example.sh "你好"
   ```
