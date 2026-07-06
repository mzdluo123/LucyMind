//! Host 抽象层:定义「命令在哪执行、文件在哪读写」的统一接口。
//!
//! - [`Host`] trait:命令执行 + 文件操作 + 路径规范化。
//! - [`LocalHost`]:本机实现(封装 `std::process::Command` / `std::fs`)。
//! - `MockHost`:测试替身(内存记录调用 + 可配置返回值)。
//! - [`WslHost`]:通过 `wsl.exe` 在 WSL 内执行(Windows 上的 Linux 环境)。
//!
//! 所有 git/hook/config 操作都通过 `&dyn Host` 间接调用,
//! 让同一套逻辑既能跑在本机,也能跑在 WSL(未来扩展 SSH)。

use std::path::{Path, PathBuf};
use std::process::Command;

// ───────────────────────────── 命令/输出/错误 ─────────────────────────────

/// 目录条目排序:目录在前,同类按名称排序(不区分大小写)。
fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
}

/// 一条待执行的命令(program + args + 可选 cwd + env)。
#[derive(Debug, Clone)]
pub struct HostCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

/// 命令执行输出(stdout + stderr + 退出状态)。
#[derive(Debug, Clone)]
pub struct HostOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

/// 目录条目(名称 + 是否目录)。用于文件选择器浏览。
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Host 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("命令失败: {cmd}\n{stderr}")]
    Command { cmd: String, stderr: String },

    #[error("路径不存在: {0}")]
    NotFound(PathBuf),
}

// ───────────────────────────── Host trait ─────────────────────────────

/// 抽象「命令在哪执行、文件在哪读写」。
///
/// - `LocalHost`:本机(`std::process::Command` / `std::fs`)。
/// - `WslHost`:通过 `wsl.exe` 在 WSL 内执行。
/// - `MockHost`:测试替身。
///
/// 所有方法要求 `&self`(不可变)——实现内部用 `Mutex` 管理可变状态(如 MockHost)。
/// trait 要求 `Send + Sync` 以便在 GPUI 的后台线程中使用(`Arc<dyn Host>` clone)。
pub trait Host: Send + Sync {
    /// 执行一个程序 + 参数(无 shell 解析),返回 stdout/stderr/exit code。
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError>;

    /// 执行一条 shell 命令字符串(`sh -c` / `cmd /C`),cwd = worktree,
    /// 注入 env 环境变量。用于 hook 命令执行。
    fn run_shell(
        &self,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<HostOutput, HostError>;

    /// 规范化路径(消解 `.`、`..`、符号链接)。
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError>;

    /// 路径是否存在。
    fn exists(&self, path: &Path) -> bool;

    /// 读文件全文为字符串。
    fn read_to_string(&self, path: &Path) -> Result<String, HostError>;

    /// 写文件(覆盖)。
    fn write(&self, path: &Path, content: &str) -> Result<(), HostError>;

    /// 复制文件。
    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError>;

    /// 递归创建目录(已存在则 no-op)。
    fn create_dir_all(&self, path: &Path) -> Result<(), HostError>;

    /// 列出目录下的条目(名称 + 是否目录),隐藏文件(以 `.` 开头)不返回。
    /// 目录排在文件前面,同类按名称排序。用于文件选择器浏览。
    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, HostError>;

    /// 返回终端 spawn 用的 (program, args)。
    /// `None` = 系统默认 shell(alacritty tty 层决定);
    /// `Some` = 指定程序(如 `wsl.exe --cd <cwd>`)。
    fn default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)>;

    /// 返回带环境变量注入的终端 spawn 命令。
    /// 非 WSL 环境返回 `None`(env 由 PTY 直接设置);
    /// WSL 环境返回 `Some(("wsl.exe", ["--cd", cwd, "--", "env", "K=V", ..., "/bin/sh"]))`
    /// (PTY env 不会跨 Windows→WSL 边界,需编入命令行)。
    fn shell_with_env(&self, cwd: &Path, env: &[(String, String)])
        -> Option<(String, Vec<String>)>;

    /// 是否是远程 Host(WSL/SSH)。app 层据此调整 UI(如隐藏本地 shell 选项)。
    fn is_remote(&self) -> bool;
}

// ───────────────────────────── LocalHost ─────────────────────────────

/// 本机 Host:封装 `std::process::Command` / `std::fs`。
///
/// 零大小 struct(ZST),`Clone + Copy` trivially。
/// 行为与改造前完全一致(回归零风险)。
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalHost;

impl LocalHost {
    /// 构造平台对应的 shell 执行命令。Unix: `sh -c`;Windows: `cmd /C`。
    fn shell_command(cmd: &str) -> Command {
        #[cfg(unix)]
        {
            let mut c = Command::new("sh");
            c.arg("-c").arg(cmd);
            c
        }
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        }
    }

    /// 剥掉 Windows verbatim 路径前缀(`\\?\` / `\\?\UNC\`),其余平台原样返回。
    fn strip_verbatim_prefix(p: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let s = p.to_string_lossy();
            if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{rest}"));
            }
            if let Some(rest) = s.strip_prefix(r"\\?\") {
                return PathBuf::from(rest);
            }
        }
        let _ = p;
        p.to_path_buf()
    }
}

impl Host for LocalHost {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError> {
        let mut c = Command::new(&cmd.program);
        if let Some(cwd) = &cmd.cwd {
            c.current_dir(cwd);
        }
        c.args(&cmd.args);
        for (k, v) in &cmd.env {
            c.env(k, v);
        }
        let output = c.output()?;
        Ok(HostOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    fn run_shell(
        &self,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<HostOutput, HostError> {
        let mut command = Self::shell_command(cmd);
        command.current_dir(cwd);
        for (k, v) in env {
            command.env(k, v);
        }
        let output = command.output()?;
        Ok(HostOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        let c = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Ok(Self::strip_verbatim_prefix(&c))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        std::fs::read_to_string(path).map_err(Into::into)
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), HostError> {
        std::fs::write(path, content).map_err(Into::into)
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        std::fs::copy(from, to).map(|_| ()).map_err(Into::into)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), HostError> {
        std::fs::create_dir_all(path).map_err(Into::into)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, HostError> {
        let mut entries: Vec<DirEntry> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return None;
                }
                let is_dir = e.file_type().ok().map(|t| t.is_dir()).unwrap_or(false);
                Some(DirEntry { name, is_dir })
            })
            .collect();
        sort_entries(&mut entries);
        Ok(entries)
    }

    fn default_shell(&self, _cwd: &Path) -> Option<(String, Vec<String>)> {
        None
    }

    fn shell_with_env(
        &self,
        _cwd: &Path,
        _env: &[(String, String)],
    ) -> Option<(String, Vec<String>)> {
        None
    }

    fn is_remote(&self) -> bool {
        false
    }
}

// ───────────────────────────── WslHost ─────────────────────────────

/// WSL Host:通过 `wsl.exe` 在 WSL Linux 环境内执行命令/文件操作。
///
/// Phase 1 用默认发行版(不指定 `--distribution`)。
/// 所有路径用 Linux 格式(`/home/...`,`/` 分隔)。
#[derive(Debug, Clone, Default)]
pub struct WslHost {
    /// 可选发行版名(None = 默认发行版)。
    pub distro: Option<String>,
}

impl WslHost {
    /// 构造 `wsl.exe` 的基础 Command(加 distro flag 如有)。
    fn wsl_command(&self) -> Command {
        let mut c = Command::new("wsl.exe");
        if let Some(d) = &self.distro {
            c.arg("--distribution").arg(d);
        }
        c
    }

    /// 单引号转义:把值用单引号包裹,内部 `'` 转义为 `'\''`。
    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    /// 构造 `export K='V';` 前缀(用于 run_shell 的 env 注入)。
    fn env_exports(env: &[(String, String)]) -> String {
        let mut s = String::new();
        for (k, v) in env {
            s.push_str("export ");
            s.push_str(k);
            s.push('=');
            s.push_str(&Self::shell_quote(v));
            s.push_str("; ");
        }
        s
    }
}

impl Host for WslHost {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError> {
        let mut c = self.wsl_command();
        if let Some(cwd) = &cmd.cwd {
            c.arg("--cd").arg(cwd);
        }
        c.arg("--");
        // env 用 `env K=V` 前缀注入(-- 后、program 前)。
        if !cmd.env.is_empty() {
            c.arg("env");
            for (k, v) in &cmd.env {
                c.arg(format!("{k}={v}"));
            }
        }
        c.arg(&cmd.program);
        c.args(&cmd.args);
        let output = c.output()?;
        Ok(HostOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    fn run_shell(
        &self,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<HostOutput, HostError> {
        let exports = Self::env_exports(env);
        let full_cmd = format!("{exports}{cmd}");
        let mut c = self.wsl_command();
        c.arg("--cd").arg(cwd);
        c.arg("--");
        c.arg("/bin/sh").arg("-c").arg(&full_cmd);
        let output = c.output()?;
        Ok(HostOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        let mut c = self.wsl_command();
        c.arg("--").arg("realpath").arg(path);
        let output = c.output()?;
        if output.status.success() {
            Ok(PathBuf::from(
                String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            ))
        } else {
            // realpath 失败(路径不存在)→ 回退原值(与 LocalHost 一致)
            Ok(path.to_path_buf())
        }
    }

    fn exists(&self, path: &Path) -> bool {
        let mut c = self.wsl_command();
        c.arg("--").arg("test").arg("-e").arg(path);
        c.output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        let mut c = self.wsl_command();
        c.arg("--").arg("cat").arg(path);
        let output = c.output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(HostError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cat 失败: {}", String::from_utf8_lossy(&output.stderr)),
            )))
        }
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), HostError> {
        use std::io::Write;
        let mut c = self.wsl_command();
        c.arg("--").arg("tee").arg(path);
        let mut child = c.stdin(std::process::Stdio::piped()).spawn()?;
        {
            let stdin = child.stdin.as_mut().expect("piped stdin");
            stdin.write_all(content.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(HostError::Io(std::io::Error::other(format!(
                "tee 退出码 {:?}",
                status.code()
            ))));
        }
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        let mut c = self.wsl_command();
        c.arg("--").arg("cp").arg(from).arg(to);
        let output = c.output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(HostError::Io(std::io::Error::other(format!(
                "cp 失败: {}",
                String::from_utf8_lossy(&output.stderr)
            ))))
        }
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), HostError> {
        let mut c = self.wsl_command();
        c.arg("--").arg("mkdir").arg("-p").arg(path);
        let output = c.output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(HostError::Io(std::io::Error::other(format!(
                "mkdir 失败: {}",
                String::from_utf8_lossy(&output.stderr)
            ))))
        }
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, HostError> {
        // ls -1p:每行一个条目,目录末尾加 /。
        let mut c = self.wsl_command();
        c.arg("--").arg("ls").arg("-1p").arg(path);
        let output = c.output()?;
        if !output.status.success() {
            return Err(HostError::Io(std::io::Error::other(format!(
                "ls 失败: {}",
                String::from_utf8_lossy(&output.stderr)
            ))));
        }
        let mut entries: Vec<DirEntry> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let is_dir = line.ends_with('/');
                let name = line.trim_end_matches('/').to_string();
                if name.is_empty() || name.starts_with('.') {
                    return None;
                }
                Some(DirEntry { name, is_dir })
            })
            .collect();
        sort_entries(&mut entries);
        Ok(entries)
    }

    fn default_shell(&self, cwd: &Path) -> Option<(String, Vec<String>)> {
        Some((
            "wsl.exe".to_string(),
            vec!["--cd".to_string(), cwd.to_string_lossy().into_owned()],
        ))
    }

    fn shell_with_env(
        &self,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Option<(String, Vec<String>)> {
        // wsl.exe --cd <cwd> -- env K=V K=V ... /bin/sh
        // PTY env 不跨 Windows→WSL 边界,故把 env 编入命令行。
        let mut args = vec!["--cd".to_string(), cwd.to_string_lossy().into_owned()];
        args.push("--".to_string());
        if !env.is_empty() {
            args.push("env".to_string());
            for (k, v) in env {
                args.push(format!("{k}={v}"));
            }
        }
        args.push("/bin/sh".to_string());
        Some(("wsl.exe".to_string(), args))
    }

    fn is_remote(&self) -> bool {
        true
    }
}

// ───────────────────────────── MockHost(测试替身) ─────────────────────────────

/// 内存 Host:记录所有调用,可配置返回值。测试专用。
///
/// - `run` / `run_shell`:记录调用参数,返回 `push_output` 预装的结果;
///   未预装时返回成功空输出。
/// - 文件操作:用内存 `HashMap<PathBuf, String>`。
/// - `canonicalize`:返回输入路径(不解析)。
/// - `default_shell`:返回 `None`(系统默认 shell)。
/// - `is_remote`:返回 `false`。
#[cfg(test)]
pub struct MockHost {
    /// 记录所有 `run` 调用。
    pub commands: std::sync::Mutex<Vec<HostCommand>>,
    /// 记录所有 `run_shell` 调用(cwd, cmd, env)。
    #[allow(clippy::type_complexity)]
    pub shell_commands: std::sync::Mutex<Vec<(PathBuf, String, Vec<(String, String)>)>>,
    /// 预装的 `run` 返回值(按 program 前缀匹配)。
    pub outputs: std::sync::Mutex<Vec<(String, HostOutput)>>,
    /// 内存文件系统。
    pub files: std::sync::Mutex<std::collections::HashMap<PathBuf, String>>,
}

#[cfg(test)]
impl MockHost {
    pub fn new() -> Self {
        Self {
            commands: std::sync::Mutex::new(Vec::new()),
            shell_commands: std::sync::Mutex::new(Vec::new()),
            outputs: std::sync::Mutex::new(Vec::new()),
            files: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 预装一个 `run` 返回值:当 `run` 的 program 匹配 `program_prefix` 时返回此输出。
    pub fn push_output(&self, program_prefix: &str, stdout: &str, success: bool) {
        self.outputs.lock().unwrap().push((
            program_prefix.to_string(),
            HostOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                success,
                exit_code: if success { Some(0) } else { Some(1) },
            },
        ));
    }

    /// 预装一个文件(供 `read_to_string` 读取)。
    pub fn insert_file(&self, path: impl AsRef<Path>, content: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.as_ref().to_path_buf(), content.to_string());
    }
}

#[cfg(test)]
impl Default for MockHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Host for MockHost {
    fn run(&self, cmd: HostCommand) -> Result<HostOutput, HostError> {
        self.commands.lock().unwrap().push(cmd.clone());
        // 按预装输出匹配:找第一个 program 前缀匹配的。
        let outputs = self.outputs.lock().unwrap();
        for (prefix, out) in outputs.iter() {
            if cmd.program.starts_with(prefix) {
                return Ok(out.clone());
            }
        }
        // 默认:成功空输出。
        Ok(HostOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            exit_code: Some(0),
        })
    }

    fn run_shell(
        &self,
        cwd: &Path,
        cmd: &str,
        env: &[(String, String)],
    ) -> Result<HostOutput, HostError> {
        self.shell_commands.lock().unwrap().push((
            cwd.to_path_buf(),
            cmd.to_string(),
            env.to_vec(),
        ));
        Ok(HostOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            exit_code: Some(0),
        })
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HostError> {
        Ok(path.to_path_buf())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HostError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| HostError::NotFound(path.to_path_buf()))
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), HostError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        let content = self.read_to_string(from)?;
        self.write(to, &content)
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), HostError> {
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, HostError> {
        let files = self.files.lock().unwrap();
        let prefix = path.to_path_buf();
        let mut entries: Vec<DirEntry> = std::collections::HashSet::new().into_iter().collect();
        for key in files.keys() {
            if let Ok(rel) = key.strip_prefix(&prefix) {
                // 第一级子条目
                if let Some(first) = rel.components().next() {
                    let name = first.as_os_str().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let is_dir = rel.components().count() > 1;
                    entries.push(DirEntry { name, is_dir });
                }
            }
        }
        // 去重(同名条目可能同时有文件和目录标记)
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries.dedup_by(|a, b| a.name == b.name);
        sort_entries(&mut entries);
        Ok(entries)
    }

    fn default_shell(&self, _cwd: &Path) -> Option<(String, Vec<String>)> {
        None
    }

    fn shell_with_env(
        &self,
        _cwd: &Path,
        _env: &[(String, String)],
    ) -> Option<(String, Vec<String>)> {
        None
    }

    fn is_remote(&self) -> bool {
        false
    }
}

// ───────────────────────────── 测试 ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_host_run_git_version() {
        // 真实进程:git --version 应成功,stdout 含 "git version"。
        let host = LocalHost;
        let out = host
            .run(HostCommand {
                program: "git".into(),
                args: vec!["--version".into()],
                cwd: None,
                env: vec![],
            })
            .expect("git --version");
        assert!(out.success, "git --version failed: {}", out.stderr);
        assert!(out.stdout.contains("git version"), "stdout: {}", out.stdout);
    }

    #[test]
    fn local_host_run_shell_echo() {
        let host = LocalHost;
        let cwd = std::env::current_dir().unwrap();
        let out = host.run_shell(&cwd, "echo hello", &[]).expect("echo hello");
        assert!(out.success);
        assert!(out.stdout.contains("hello"), "stdout: {}", out.stdout);
    }

    #[test]
    fn local_host_canonicalize_current_dir() {
        let host = LocalHost;
        let dir = std::env::current_dir().unwrap();
        let c = host.canonicalize(&dir).expect("canonicalize");
        // canonicalize 后应不含 \\?\ 前缀(Windows)。
        let s = c.to_string_lossy();
        assert!(!s.starts_with(r"\\?\"), "should strip verbatim: {s}");
    }

    #[test]
    fn local_host_exists() {
        let host = LocalHost;
        let dir = std::env::current_dir().unwrap();
        assert!(host.exists(&dir));
        assert!(!host.exists(Path::new("/this/does/not/exist/hopefully")));
    }

    #[test]
    fn local_host_read_write_roundtrip() {
        let host = LocalHost;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        host.write(&path, "hello world").unwrap();
        let content = host.read_to_string(&path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn local_host_copy() {
        let host = LocalHost;
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        host.write(&src, "copy me").unwrap();
        host.copy(&src, &dst).unwrap();
        assert_eq!(host.read_to_string(&dst).unwrap(), "copy me");
    }

    #[test]
    fn local_host_create_dir_all() {
        let host = LocalHost;
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b/c");
        host.create_dir_all(&nested).unwrap();
        assert!(host.exists(&nested));
    }

    #[test]
    fn local_host_list_dir() {
        let host = LocalHost;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file1.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("file2.rs"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();

        let entries = host.list_dir(dir.path()).unwrap();
        // 隐藏文件不返回。
        assert!(!entries.iter().any(|e| e.name == ".hidden"));
        // 目录在前。
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "subdir");
        // 文件按名称排序。
        assert_eq!(entries[1].name, "file1.txt");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[2].name, "file2.rs");
        assert!(!entries[2].is_dir);
    }

    #[test]
    fn local_host_list_dir_nonexistent() {
        let host = LocalHost;
        let result = host.list_dir(Path::new("/this/does/not/exist/hopefully"));
        assert!(result.is_err());
    }

    #[test]
    fn local_host_list_dir_empty() {
        let host = LocalHost;
        let dir = tempfile::tempdir().unwrap();
        let entries = host.list_dir(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn local_host_default_shell_is_none() {
        let host = LocalHost;
        assert!(host.default_shell(Path::new("/tmp")).is_none());
    }

    #[test]
    fn local_host_is_remote_false() {
        assert!(!LocalHost.is_remote());
    }

    #[test]
    fn mock_host_records_run_command() {
        let host = MockHost::new();
        host.run(HostCommand {
            program: "git".into(),
            args: vec!["status".into()],
            cwd: None,
            env: vec![],
        })
        .unwrap();
        let cmds = host.commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "git");
        assert_eq!(cmds[0].args, vec!["status"]);
    }

    #[test]
    fn mock_host_records_run_shell() {
        let host = MockHost::new();
        host.run_shell(
            Path::new("/wt"),
            "npm install",
            &[("WORKTREE_PATH".into(), "/wt".into())],
        )
        .unwrap();
        let shells = host.shell_commands.lock().unwrap();
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].0, Path::new("/wt"));
        assert_eq!(shells[0].1, "npm install");
        assert_eq!(shells[0].2.len(), 1);
        assert_eq!(shells[0].2[0].0, "WORKTREE_PATH");
    }

    #[test]
    fn mock_host_push_output_matches() {
        let host = MockHost::new();
        host.push_output("git", "worktree /home/u/proj\nHEAD abc\n", true);
        let out = host
            .run(HostCommand {
                program: "git".into(),
                args: vec!["worktree".into(), "list".into()],
                cwd: None,
                env: vec![],
            })
            .unwrap();
        assert!(out.success);
        assert!(out.stdout.contains("worktree /home/u/proj"));
    }

    #[test]
    fn mock_host_files_roundtrip() {
        let host = MockHost::new();
        host.insert_file(
            "/repo/.worktree.toml",
            "[worktree]\ndefault_base = \"main\"\n",
        );
        let content = host
            .read_to_string(Path::new("/repo/.worktree.toml"))
            .unwrap();
        assert!(content.contains("default_base"));
    }

    #[test]
    fn mock_host_copy_uses_files() {
        let host = MockHost::new();
        host.insert_file("/src/.env", "SECRET=1\n");
        host.copy(Path::new("/src/.env"), Path::new("/dst/.env"))
            .unwrap();
        assert_eq!(
            host.read_to_string(Path::new("/dst/.env")).unwrap(),
            "SECRET=1\n"
        );
    }

    #[test]
    fn mock_host_canonicalize_returns_input() {
        let host = MockHost::new();
        let p = Path::new("/some/path/.");
        let c = host.canonicalize(p).unwrap();
        assert_eq!(c, p);
    }

    #[test]
    fn mock_host_exists_checks_files() {
        let host = MockHost::new();
        host.insert_file("/file.txt", "content");
        assert!(host.exists(Path::new("/file.txt")));
        assert!(!host.exists(Path::new("/nope.txt")));
    }

    #[test]
    fn mock_host_list_dir() {
        let host = MockHost::new();
        host.insert_file("/repo/file1.txt", "");
        host.insert_file("/repo/file2.txt", "");
        host.insert_file("/repo/.hidden", "");
        host.insert_file("/repo/sub/a.txt", "");
        host.insert_file("/other/x.txt", "");

        let entries = host.list_dir(Path::new("/repo")).unwrap();
        // sub 是目录(有子文件),file1/file2 是文件,.hidden 不返回。
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "sub");
        assert_eq!(entries[1].name, "file1.txt");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[2].name, "file2.txt");
    }

    // ───────────────────────────── WslHost 命令构造测试 ─────────────────────────────

    #[test]
    fn wsl_host_default_shell() {
        let host = WslHost::default();
        let shell = host.default_shell(Path::new("/home/user/wt"));
        assert_eq!(
            shell,
            Some((
                "wsl.exe".into(),
                vec!["--cd".into(), "/home/user/wt".into()]
            ))
        );
    }

    #[test]
    fn wsl_host_is_remote_true() {
        assert!(WslHost::default().is_remote());
    }

    #[test]
    fn wsl_host_shell_quote_simple() {
        assert_eq!(WslHost::shell_quote("hello"), "'hello'");
    }

    #[test]
    fn wsl_host_shell_quote_with_single_quote() {
        // 单引号转义:it's → 'it'\''s'
        assert_eq!(WslHost::shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn wsl_host_env_exports_simple() {
        let exports = WslHost::env_exports(&[("WORKTREE_PATH".into(), "/home/wt".into())]);
        assert_eq!(exports, "export WORKTREE_PATH='/home/wt'; ");
    }

    #[test]
    fn wsl_host_env_exports_quote_value() {
        let exports = WslHost::env_exports(&[("VAR".into(), "it's here".into())]);
        assert_eq!(exports, "export VAR='it'\\''s here'; ");
    }

    #[test]
    fn wsl_host_shell_with_env_injects_vars() {
        let host = WslHost::default();
        let shell = host
            .shell_with_env(
                Path::new("/home/user/wt"),
                &[
                    ("TERM".into(), "xterm-256color".into()),
                    ("WORKTREE_PATH".into(), "/home/user/wt".into()),
                ],
            )
            .expect("WslHost returns Some");
        assert_eq!(shell.0, "wsl.exe");
        assert_eq!(
            shell.1,
            vec![
                "--cd".to_string(),
                "/home/user/wt".to_string(),
                "--".to_string(),
                "env".to_string(),
                "TERM=xterm-256color".to_string(),
                "WORKTREE_PATH=/home/user/wt".to_string(),
                "/bin/sh".to_string(),
            ]
        );
    }

    #[test]
    fn wsl_host_shell_with_env_no_env_still_has_sh() {
        let host = WslHost::default();
        let shell = host
            .shell_with_env(Path::new("/home/user/wt"), &[])
            .expect("WslHost returns Some");
        // 即使无 env,仍应有 -- /bin/sh(无 env 前缀)。
        assert_eq!(shell.0, "wsl.exe");
        assert!(shell.1.contains(&"--".to_string()));
        assert!(shell.1.contains(&"/bin/sh".to_string()));
        assert!(
            !shell.1.contains(&"env".to_string()),
            "should not have env prefix when empty"
        );
    }

    #[test]
    fn local_host_shell_with_env_is_none() {
        let host = LocalHost;
        assert!(host
            .shell_with_env(Path::new("/tmp"), &[("TERM".into(), "xterm".into())])
            .is_none());
    }
}
