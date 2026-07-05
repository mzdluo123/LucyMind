//! 按键 → 终端字节序列编码。
//!
//! alacritty 内核**不负责**键盘编码——调用方(我们)要把按键编成字节送 PTY。
//! 这里定义中性输入类型(不依赖 GPUI,便于单测),app 层把 GPUI 事件转成它。
//! 完整编码(功能键/APP_CURSOR/kitty 等)见 U7;此处先覆盖 MVP 常用键。

/// 中性修饰键状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// 中性按键(app 层从 GPUI KeyDownEvent 转成它)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
}

/// 把一个按键编码成要写入 PTY 的字节。U7 会扩展(功能键、光标应用模式等)。
pub fn encode(key: &Key, mods: Mods) -> Vec<u8> {
    match key {
        Key::Char(c) => encode_char(*c, mods),
        Key::Enter => vec![b'\r'],
        Key::Backspace => vec![0x7f],
        Key::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec() // Shift-Tab = CBT
            } else {
                vec![b'\t']
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => csi_or_ss3(b'A'),
        Key::Down => csi_or_ss3(b'B'),
        Key::Right => csi_or_ss3(b'C'),
        Key::Left => csi_or_ss3(b'D'),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
    }
}

/// 普通字符编码:Ctrl 组合成控制码,Alt 加 ESC 前缀。
fn encode_char(c: char, mods: Mods) -> Vec<u8> {
    // Ctrl+字母 → 控制码(Ctrl-A = 0x01 ...)。
    if mods.ctrl {
        if let Some(ctrl) = ctrl_code(c) {
            return if mods.alt {
                vec![0x1b, ctrl]
            } else {
                vec![ctrl]
            };
        }
    }

    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf).as_bytes().to_vec();
    if mods.alt {
        let mut out = vec![0x1b];
        out.extend_from_slice(&s);
        out
    } else {
        s
    }
}

/// Ctrl+字母/常见符号 → 控制码。
fn ctrl_code(c: char) -> Option<u8> {
    let upper = c.to_ascii_uppercase();
    match upper {
        '@'..='_' => Some((upper as u8) & 0x1f), // @ A-Z [ \ ] ^ _
        'a'..='z' => Some((c.to_ascii_uppercase() as u8) & 0x1f),
        ' ' => Some(0), // Ctrl-Space = NUL
        _ => None,
    }
}

/// 方向键:MVP 用普通 CSI(`ESC [ A`)。U7 会根据 APP_CURSOR 模式切成 SS3(`ESC O A`)。
fn csi_or_ss3(final_byte: u8) -> Vec<u8> {
    vec![0x1b, b'[', final_byte]
}

/// 粘贴文本编码。`bracketed` 为 true 时用 bracketed-paste 包裹。
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_char() {
        assert_eq!(encode(&Key::Char('a'), Mods::default()), b"a");
    }

    #[test]
    fn ctrl_c_is_etx() {
        let mods = Mods {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(encode(&Key::Char('c'), mods), vec![0x03]);
    }

    #[test]
    fn alt_char_has_esc_prefix() {
        let mods = Mods {
            alt: true,
            ..Default::default()
        };
        assert_eq!(encode(&Key::Char('x'), mods), vec![0x1b, b'x']);
    }

    #[test]
    fn enter_and_backspace() {
        assert_eq!(encode(&Key::Enter, Mods::default()), b"\r");
        assert_eq!(encode(&Key::Backspace, Mods::default()), vec![0x7f]);
    }

    #[test]
    fn shift_tab_is_cbt() {
        let mods = Mods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(encode(&Key::Tab, mods), b"\x1b[Z");
    }

    #[test]
    fn arrows_csi() {
        assert_eq!(encode(&Key::Up, Mods::default()), b"\x1b[A");
        assert_eq!(encode(&Key::Left, Mods::default()), b"\x1b[D");
    }

    #[test]
    fn bracketed_paste_wraps() {
        let out = encode_paste("hi", true);
        assert_eq!(out, b"\x1b[200~hi\x1b[201~");
    }

    #[test]
    fn utf8_char() {
        // 中文字符编码为多字节 UTF-8。
        assert_eq!(encode(&Key::Char('中'), Mods::default()), "中".as_bytes());
    }
}
