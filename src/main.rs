use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEvent, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Clear},
};
use serde_json::Value;
use std::{io, time::Duration, fmt::Write};
use std::fs;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use copypasta::{ClipboardContext, ClipboardProvider};
use tokio::sync::mpsc;
use pulldown_cmark::{Parser, Event as MarkdownEvent, Tag};

const AVAILABLE_MODELS: [&str; 21] = [
    "ernie-4.0-8k-latest",
    "ernie-4.0-8k-preview",
    "ernie-4.0-8k",
    "ernie-4.0-turbo-8k-latest",
    "ernie-4.0-turbo-8k-preview",
    "ernie-4.0-turbo-8k",
    "ernie-4.0-turbo-128k",
    "ernie-3.5-8k-preview",
    "ernie-3.5-8k",
    "ernie-3.5-128k",
    "ernie-speed-8k",
    "ernie-speed-128k",
    "ernie-speed-pro-128k",
    "ernie-lite-8k",
    "ernie-lite-pro-128k",
    "ernie-tiny-8k",
    "ernie-char-8k",
    "ernie-char-fiction-8k",
    "ernie-novel-8k",
    "deepseek-v3",
    "deepseek-r1"
];

#[derive(Clone)]
struct Message {
    role: String,
    content: String,
    timestamp: String,
}

impl Message {
    fn format_content(&self) -> String {
        // Simply return the content without any filtering
        self.content.clone()
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    auth_token: String,
}

impl Config {
    fn load() -> Self {
        let config_path = get_config_path();
        if let Ok(contents) = fs::read_to_string(config_path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Config::default()
        }
    }

    fn save(&self) -> Result<()> {
        let config_path = get_config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string(self)?;
        fs::write(config_path, contents)?;
        Ok(())
    }
}

fn get_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("llm_tui");
    path.push("config.json");
    path
}

struct App {
    input: String,
    response: String,
    api_url: String,
    auth_token: String,
    show_config: bool,
    show_help: bool,
    config_input: String,
    visible_token: String,
    active_box: usize, // 0: input, 1: response
    history: Vec<Message>,
    config: Config,
    scroll_offset: u16,  // Add this for scrolling
    clipboard: ClipboardContext,
    response_area: Option<Rect>,  // Add this field
    is_loading: bool,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    input_history: Vec<String>,
    input_history_index: Option<usize>,
    current_input: String,  // Store current input when navigating history
    current_model: String,
    show_model_select: bool,
    model_select_index: usize,
}

impl App {
    fn new() -> App {
        let config = Config::load();
        let (tx, rx) = mpsc::channel(100);  // Create channel with buffer size 100
        App {
            input: String::new(),
            response: String::new(),
            api_url: String::from("https://qianfan.baidubce.com/v2/chat/completions"),
            auth_token: config.auth_token.clone(),
            show_config: false,
            show_help: false,
            config_input: String::new(),
            visible_token: config.auth_token.clone(),
            active_box: 0,
            history: Vec::new(),
            scroll_offset: 0,
            config,
            clipboard: ClipboardContext::new().unwrap_or_else(|_| panic!("无法初始化剪贴板")),
            response_area: None,
            is_loading: false,
            tx,
            rx,
            input_history: Vec::new(),
            input_history_index: None,
            current_input: String::new(),
            current_model: String::from("deepseek-r1"),  // Default model
            show_model_select: false,
            model_select_index: AVAILABLE_MODELS.len() - 1,  // Default to deepseek-r1
        }
    }

    fn format_curl_command(&self, payload: &serde_json::Value) -> String {
        let json_str = serde_json::to_string_pretty(payload).unwrap_or_default()
            .replace("\n", "\n    ");
        
        format!(
            "curl -X POST '{}' -H 'Content-Type: application/json' -H 'Authorization: Bearer {}' -d '{}'",
            self.api_url,
            self.auth_token,
            json_str
        )
    }

    fn get_content_height(&self) -> u16 {
        self.format_history().lines().count() as u16
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16) {
        let content_height = self.get_content_height();
        if content_height > viewport_height {
            self.scroll_offset = content_height - viewport_height;
        } else {
            self.scroll_offset = 0;
        }
    }

    fn navigate_history(&mut self, up: bool) {
        if self.input_history.is_empty() {
            return;
        }

        if let Some(index) = self.input_history_index {
            if up && index > 0 {
                self.input_history_index = Some(index - 1);
            } else if !up && index < self.input_history.len() - 1 {
                self.input_history_index = Some(index + 1);
            }
        } else {
            // Save current input when starting navigation
            self.current_input = self.input.clone();
            self.input_history_index = Some(if up {
                self.input_history.len() - 1
            } else {
                0
            });
        }

        // Update input with historical message
        if let Some(index) = self.input_history_index {
            self.input = self.input_history[index].clone();
        }
    }

    async fn send_request(&mut self) -> Result<()> {
        if self.auth_token.is_empty() {
            self.handle_new_message(Message {
                role: "system".to_string(),
                content: "错误: 请先配置API认证令牌".to_string(),
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            }).await;
            return Ok(());
        }

        // Save to input history if not empty and not duplicate
        if !self.input.trim().is_empty() {
            if self.input_history.last() != Some(&self.input) {
                self.input_history.push(self.input.clone());
            }
        }

        // Reset history navigation
        self.input_history_index = None;
        self.current_input.clear();

        // Clone all needed values
        let api_url = self.api_url.clone();
        let auth_token = self.auth_token.clone();
        let tx = self.tx.clone();
        let current_model = self.current_model.clone();
        let user_input = self.input.clone();

        self.input.clear();
        
        // Add user message to history
        self.handle_new_message(Message {
            role: "user".to_string(),
            content: user_input.clone(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        }).await;

        self.is_loading = true;
        
        // Add loading message
        self.handle_new_message(Message {
            role: "system".to_string(),
            content: "正在等待响应...".to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        }).await;

        // Spawn the request as a separate task
        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .danger_accept_invalid_certs(true)
                .no_proxy()
                .build()
                .unwrap_or_default();

            let payload = serde_json::json!({
                "model": current_model,  // Use cloned value
                "messages": [
                    {
                        "role": "user",
                        "content": user_input
                    }
                ]
            });

            // Send request
            match client
                .post(&api_url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", auth_token))
                .json(&payload)
                .send()
                .await {
                    Ok(response) => {
                        match response.text().await {
                            Ok(text) => {
                                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                    if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
                                        let _ = tx.send(Message {
                                            role: "assistant".to_string(),
                                            content: content.to_string(),
                                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                        }).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Message {
                                    role: "system".to_string(),
                                    content: format!("响应解析错误: {}", e),
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                }).await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Message {
                            role: "system".to_string(),
                            content: format!("请求错误: {}", e),
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        }).await;
                    }
                }
        });

        Ok(())
    }

    fn get_help_text(&self) -> String {
        let mut help = String::new();
        let _ = writeln!(help, "帮助菜单:");
        let _ = writeln!(help, "--------");
        let _ = writeln!(help, "Alt+H    - 显示此帮助菜单");
        let _ = writeln!(help, "Alt+C    - 配置认证令牌");
        let _ = writeln!(help, "Alt+M    - 选择模型");
        let _ = writeln!(help, "Alt+Y    - 复制最后一条AI回复");
        let _ = writeln!(help, "Tab      - 切换输入框和历史框");
        let _ = writeln!(help, "↑/↓      - 在历史框中滚动");
        let _ = writeln!(help, "Enter    - 发送请求");
        let _ = writeln!(help, "Ctrl+C   - 退出程序");
        let _ = writeln!(help, "Esc      - 退出程序或关闭弹窗");
        help
    }

    fn wrap_text(&self, text: &str, width: usize) -> String {
        let mut wrapped = String::new();
        for line in text.lines() {
            let mut current_line = String::new();
            for word in line.split_whitespace() {
                if current_line.len() + word.len() + 1 > width {
                    if !current_line.is_empty() {
                        wrapped.push_str(&current_line);
                        wrapped.push('\n');
                        current_line.clear();
                    }
                    // If word is longer than width, split it
                    if word.len() > width {
                        let mut chars = word.chars().peekable();
                        while chars.peek().is_some() {
                            let chunk: String = chars.by_ref().take(width).collect();
                            wrapped.push_str(&chunk);
                            wrapped.push('\n');
                        }
                    } else {
                        current_line = word.to_string();
                    }
                } else {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
            }
            if !current_line.is_empty() {
                wrapped.push_str(&current_line);
                wrapped.push('\n');
            }
        }
        wrapped
    }

    fn get_content_width(&self) -> usize {
        if let Some(area) = self.response_area {
            // Subtract 4 for borders and padding (2 on each side)
            (area.width as usize).saturating_sub(4)
        } else {
            80  // Default width if area not available
        }
    }

    fn format_history(&self) -> String {
        let mut formatted = String::new();
        let width = self.get_content_width();
        
        for msg in &self.history {
            let (role_display, _) = match msg.role.as_str() {
                "user" => ("你", ""),
                "assistant" => ("AI", ""),
                _ => ("系统", ""),
            };
            
            let header = format!("[{}] {}: ", msg.timestamp, role_display);
            let content = msg.format_content();
            
            formatted.push_str(&header);
            
            // Subtract header length from available width for content
            let content_width = width.saturating_sub(header.len());
            let wrapped_content = self.wrap_text(&content, content_width);
            let indented_content = wrapped_content.lines()
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        line.to_string()
                    } else {
                        format!("{:width$}{}", "", line, width = header.len())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            
            formatted.push_str(&indented_content);
            formatted.push_str("\n\n");
        }
        formatted
    }

    fn markdown_to_styled_text(&self, markdown: &str) -> Vec<Line> {
        let width = self.get_content_width();
        let parser = Parser::new(markdown);
        let mut styled_lines = Vec::new();
        let mut current_line = Vec::new();
        let mut in_code_block = false;
        let mut list_level = 0;

        for event in parser {
            match event {
                MarkdownEvent::Start(Tag::Heading(level, _, _)) => {
                    if !current_line.is_empty() {
                        styled_lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    let marker = "#".repeat(level as usize);
                    current_line.push(Span::styled(
                        format!("{} ", marker),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    ));
                }
                MarkdownEvent::Start(Tag::CodeBlock(_)) => {
                    if !current_line.is_empty() {
                        styled_lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    in_code_block = true;
                }
                MarkdownEvent::End(Tag::CodeBlock(_)) => {
                    if !current_line.is_empty() {
                        styled_lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    in_code_block = false;
                }
                MarkdownEvent::Start(Tag::List(_)) => {
                    list_level += 1;
                }
                MarkdownEvent::End(Tag::List(_)) => {
                    list_level -= 1;
                }
                MarkdownEvent::Start(Tag::Item) => {
                    if !current_line.is_empty() {
                        styled_lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    let indent = "  ".repeat(list_level - 1);
                    current_line.push(Span::raw(format!("{}• ", indent)));
                }
                MarkdownEvent::Start(Tag::Emphasis) => {
                    current_line.push(Span::styled(
                        "",
                        Style::default().add_modifier(Modifier::ITALIC)
                    ));
                }
                MarkdownEvent::Start(Tag::Strong) => {
                    current_line.push(Span::styled(
                        "",
                        Style::default().add_modifier(Modifier::BOLD)
                    ));
                }
                MarkdownEvent::Text(text) => {
                    let style = if in_code_block {
                        Style::default()
                            .fg(Color::Cyan)
                            .bg(Color::Black)
                    } else {
                        Style::default()
                    };
                    current_line.push(Span::styled(text.to_string(), style));
                }
                MarkdownEvent::End(_) => {
                    if !matches!(event, MarkdownEvent::End(Tag::Emphasis) | MarkdownEvent::End(Tag::Strong)) {
                        if !current_line.is_empty() {
                            styled_lines.push(Line::from(current_line.clone()));
                            current_line.clear();
                        }
                    }
                }
                MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                    if !current_line.is_empty() {
                        styled_lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                _ => {}
            }
        }

        if !current_line.is_empty() {
            styled_lines.push(Line::from(current_line));
        }

        styled_lines
    }

    fn get_styled_history(&self) -> Vec<Line> {
        let mut styled_lines = Vec::new();
        
        for msg in &self.history {
            let (role_display, _) = match msg.role.as_str() {
                "user" => ("你", ""),
                "assistant" => ("AI", ""),
                _ => ("系统", ""),
            };
            
            let header = format!("[{}] {}: ", msg.timestamp, role_display);
            styled_lines.push(Line::from(vec![
                Span::styled(header, Style::default().fg(Color::Green))
            ]));

            if msg.role == "assistant" {
                let mut markdown_lines = self.markdown_to_styled_text(&msg.content);
                for line in markdown_lines.iter_mut() {
                    line.spans.insert(0, Span::raw("    "));
                }
                styled_lines.extend(markdown_lines);
            } else {
                styled_lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::raw(&msg.content)
                ]));
            }

            styled_lines.push(Line::from(""));
        }

        styled_lines
    }

    fn save_config(&mut self) -> Result<()> {
        self.config.auth_token = self.auth_token.clone();
        self.config.save()?;
        Ok(())
    }

    fn scroll(&mut self, up: bool) {
        if up {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
    }

    fn copy_to_clipboard(&mut self, text: &str) -> Result<()> {
        if let Err(e) = self.clipboard.set_contents(text.to_string()) {
            self.history.push(Message {
                role: "system".to_string(),
                content: format!("复制到剪贴板失败: {}", e),
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            });
        } else {
            self.history.push(Message {
                role: "system".to_string(),
                content: "已复制到剪贴板".to_string(),
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            });
        }
        if let Some(area) = self.response_area {
            self.scroll_to_bottom(area.height);
        }
        Ok(())
    }

    fn update_layout(&mut self, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(70),
            ])
            .split(area);
        self.response_area = Some(chunks[1]);
    }

    async fn handle_new_message(&mut self, message: Message) {
        if let Some(last) = self.history.last() {
            if last.content == "正在等待响应..." {
                self.history.pop();
            }
        }

        let is_assistant = message.role == "assistant";
        self.history.push(message);
        
        // Always scroll to bottom for new messages
        if let Some(area) = self.response_area {
            let content_height = self.get_content_height();
            if content_height > area.height {
                self.scroll_offset = content_height - area.height;
            } else {
                self.scroll_offset = 0;
            }
        }

        if is_assistant {
            self.is_loading = false;
        }
    }

    fn get_model_select_text(&self) -> String {
        let mut text = String::new();
        for (i, model) in AVAILABLE_MODELS.iter().enumerate() {
            let prefix = if i == self.model_select_index { "> " } else { "  " };
            let _ = writeln!(text, "{}{}", prefix, model);
        }
        text
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut terminal = terminal;
    let mut app = App::new();
    
    loop {
        if let Ok(message) = app.rx.try_recv() {
            app.handle_new_message(message).await;
        }

        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.show_help {
                        if matches!(key.code, KeyCode::Esc | KeyCode::Char('h')) {
                            app.show_help = false;
                        }
                    } else if app.show_config {
                        match key.code {
                            KeyCode::Enter => {
                                app.auth_token = app.config_input.clone();
                                app.visible_token = app.config_input.clone();
                                if let Err(e) = app.save_config() {
                                    app.history.push(Message {
                                        role: "system".to_string(),
                                        content: format!("配置保存错误: {}", e),
                                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                    });
                                }
                                app.show_config = false;
                                app.config_input.clear();
                            }
                            KeyCode::Char(c) => {
                                app.config_input.push(c);
                            }
                            KeyCode::Backspace => {
                                app.config_input.pop();
                            }
                            KeyCode::Esc => {
                                app.show_config = false;
                                app.config_input.clear();
                            }
                            _ => {}
                        }
                    } else if app.show_model_select {
                        match key.code {
                            KeyCode::Up => {
                                if app.model_select_index > 0 {
                                    app.model_select_index -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if app.model_select_index < AVAILABLE_MODELS.len() - 1 {
                                    app.model_select_index += 1;
                                }
                            }
                            KeyCode::Enter => {
                                app.current_model = AVAILABLE_MODELS[app.model_select_index].to_string();
                                app.show_model_select = false;
                                // Add confirmation message
                                app.history.push(Message {
                                    role: "system".to_string(),
                                    content: format!("已切换到模型: {}", app.current_model),
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                });
                            }
                            KeyCode::Esc => {
                                app.show_model_select = false;
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Enter => {
                                if app.active_box == 0 {
                                    if let Err(e) = app.send_request().await {
                                        app.response = format!("错误: {}", e);
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                app.active_box = 1 - app.active_box;
                            }
                            KeyCode::Up => {
                                if app.active_box == 1 {
                                    app.scroll(true);
                                } else {
                                    app.navigate_history(true);
                                }
                            }
                            KeyCode::Down => {
                                if app.active_box == 1 {
                                    app.scroll(false);
                                } else {
                                    if app.input_history_index.is_some() {
                                        app.navigate_history(false);
                                    } else {
                                        app.input = app.current_input.clone();
                                        app.current_input.clear();
                                    }
                                }
                            }
                            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                                break;
                            }
                            KeyCode::Char('h') if key.modifiers.contains(event::KeyModifiers::ALT) => {
                                app.show_help = true;
                            }
                            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::ALT) => {
                                app.show_config = true;
                                app.config_input = app.visible_token.clone();
                            }
                            KeyCode::Char('y') if key.modifiers.contains(event::KeyModifiers::ALT) => {
                                if app.active_box == 1 && app.history.len() > 0 {
                                    let content = app.history.iter()
                                        .rev()
                                        .find(|msg| msg.role == "assistant")
                                        .map(|msg| msg.content.clone());
                                    
                                    if let Some(content) = content {
                                        let _ = app.copy_to_clipboard(&content);
                                    }
                                }
                            }
                            KeyCode::Char('m') if key.modifiers.contains(event::KeyModifiers::ALT) => {
                                app.show_model_select = true;
                                // Find current model index
                                app.model_select_index = AVAILABLE_MODELS
                                    .iter()
                                    .position(|&m| m == app.current_model)
                                    .unwrap_or(AVAILABLE_MODELS.len() - 1);
                            }
                            KeyCode::Char(c) => {
                                if app.active_box == 0 {
                                    app.input.push(c);
                                }
                            }
                            KeyCode::Backspace => {
                                if app.active_box == 0 {
                                    app.input.pop();
                                }
                            }
                            KeyCode::Esc => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(f.size());

    app.update_layout(f.size());

    let active_border_style = Style::default()
        .fg(Color::Green);
    
    let inactive_border_style = Style::default();

    let input_title = if app.is_loading {
        "输入 (正在等待响应...)"
    } else {
        "输入 (Enter发送, Alt+C配置, Alt+H帮助)"
    };

    let input = Paragraph::new(app.input.as_str())
        .block(Block::default()
            .title(input_title)
            .borders(Borders::ALL)
            .border_style(if app.active_box == 0 { active_border_style } else { inactive_border_style }));
    f.render_widget(input, chunks[0]);

    let styled_history = app.get_styled_history();
    let response = Paragraph::new(styled_history)
        .scroll((app.scroll_offset, 0))
        .block(Block::default()
            .title("对话历史 (↑/↓滚动)")
            .borders(Borders::ALL)
            .border_style(if app.active_box == 1 { active_border_style } else { inactive_border_style }));
    f.render_widget(response, chunks[1]);

    if app.show_help {
        let area = centered_rect(60, 50, f.size());
        let help_text = app.get_help_text();
        let help_popup = Paragraph::new(help_text)
            .block(Block::default().title("帮助").borders(Borders::ALL));
        f.render_widget(Clear, area);
        f.render_widget(help_popup, area);
    }

    if app.show_config {
        let area = centered_rect(60, 20, f.size());
        let config_popup = Paragraph::new(app.config_input.as_str())
            .block(Block::default().title("输入认证令牌 (当前令牌已保存)").borders(Borders::ALL));
        f.render_widget(Clear, area);
        f.render_widget(config_popup, area);
    }

    if app.show_model_select {
        let area = centered_rect(60, 80, f.size());
        let model_text = app.get_model_select_text();
        let model_popup = Paragraph::new(model_text)
            .block(Block::default()
                .title(format!("选择模型 (当前: {})", app.current_model))
                .borders(Borders::ALL));
        f.render_widget(Clear, area);
        f.render_widget(model_popup, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
} 