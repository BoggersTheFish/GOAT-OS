//! Minimal shell (runs in user mode via node 0)

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

const SYS_WRITE: u64 = 1;
const SYS_YIELD: u64 = 2;
const SYS_SPAWN: u64 = 3;
const SYS_READ: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_LS: u64 = 6;
const SYS_CAT: u64 = 7;
const SYS_PS: u64 = 8;
const SYS_TOUCH: u64 = 9;
const SYS_MKDIR: u64 = 10;
const SYS_WRITE_F: u64 = 11;
const SYS_SHUTDOWN: u64 = 12;
const SYS_CLEAR: u64 = 13;
const SYS_POLL_KEY: u64 = 14;
const SYS_RM: u64 = 15;
const SYS_GETPID: u64 = 16;
const SYS_CHDIR: u64 = 17;
const SYS_GETCWD: u64 = 18;
const SYS_WAIT: u64 = 19;
const SYS_KILL: u64 = 20;

fn syscall0(n: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!("int 0x80", in("rax") n, lateout("rax") ret, options(nostack, preserves_flags));
    }
    ret
}

fn syscall1(n: u64, a: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!("int 0x80", in("rax") n, in("rdi") a, lateout("rax") ret, options(nostack, preserves_flags));
    }
    ret
}

fn syscall2(n: u64, a: u64, b: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!("int 0x80", in("rax") n, in("rdi") a, in("rsi") b, lateout("rax") ret, options(nostack, preserves_flags));
    }
    ret
}

fn syscall1_4(n: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("rax") n,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("rcx") d,
            lateout("rax") ret,
            options(nostack, preserves_flags)
        );
    }
    ret
}

fn do_write(s: &str) {
    syscall1_4(SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64, 0);
}

fn do_read(buf: &mut [u8]) -> usize {
    syscall1_4(SYS_READ, 0, buf.as_mut_ptr() as u64, buf.len() as u64, 0) as usize
}

fn do_yield() {
    syscall0(SYS_YIELD);
}

fn do_clear_screen() {
    syscall0(SYS_CLEAR);
}

fn do_poll_key() -> bool {
    syscall0(SYS_POLL_KEY) != 0
}

fn do_spawn() -> u64 {
    syscall0(SYS_SPAWN)
}

fn do_ls(path: &str, out: &mut [u8]) -> usize {
    syscall1_4(SYS_LS, path.as_ptr() as u64, path.len() as u64, out.as_mut_ptr() as u64, out.len() as u64) as usize
}

fn do_cat(path: &str, out: &mut [u8]) -> usize {
    syscall1_4(SYS_CAT, path.as_ptr() as u64, path.len() as u64, out.as_mut_ptr() as u64, out.len() as u64) as usize
}

fn do_ps(out: &mut [u8]) -> usize {
    syscall2(SYS_PS, out.as_mut_ptr() as u64, out.len() as u64) as usize
}

fn do_touch(path: &str) -> bool {
    syscall2(SYS_TOUCH, path.as_ptr() as u64, path.len() as u64) != 0
}

fn do_mkdir(path: &str) -> bool {
    syscall2(SYS_MKDIR, path.as_ptr() as u64, path.len() as u64) != 0
}

fn do_write_file(path: &str, data: &[u8]) -> bool {
    syscall1_4(SYS_WRITE_F, path.as_ptr() as u64, path.len() as u64, data.as_ptr() as u64, data.len() as u64) != 0
}

fn do_rm(path: &str) -> bool {
    syscall2(SYS_RM, path.as_ptr() as u64, path.len() as u64) != 0
}

fn do_getpid() -> u64 {
    syscall0(SYS_GETPID)
}

fn do_chdir(path: &str) -> bool {
    syscall2(SYS_CHDIR, path.as_ptr() as u64, path.len() as u64) != 0
}

fn do_getcwd(buf: &mut [u8]) -> usize {
    syscall2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
}

fn do_wait() -> u64 {
    syscall0(SYS_WAIT)
}

fn do_kill(pid: u32, sig: u32) -> bool {
    syscall2(SYS_KILL, pid as u64, sig as u64) != !0u64
}

fn do_exit(status: u64) -> ! {
    unsafe {
        core::arch::asm!("int 0x80", in("rax") SYS_EXIT, in("rdi") status, options(nostack, preserves_flags));
    }
    loop {
        core::hint::spin_loop();
    }
}

const HISTORY_MAX: usize = 16;
const LINE_MAX: usize = 128;

fn resolve_path(cwd: &str, p: &str) -> String {
    let p = p.trim();
    if p.starts_with('/') {
        return p.to_string();
    }
    let base = if cwd.ends_with('/') && cwd != "/" {
        cwd[..cwd.len() - 1].to_string()
    } else {
        cwd.to_string()
    };
    let mut parts: Vec<&str> = base.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    for seg in p.split('/').filter(|s| !s.is_empty()) {
        if seg == "." {
        } else if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        "/".to_string() + &parts.join("/")
    }
}

fn read_line(buf: &mut [u8], history: &[[u8; LINE_MAX]], history_len: usize, history_idx: &mut usize) -> usize {
    let mut i = 0;
    let mut esc_state = 0u8;
    *history_idx = history_len;
    while i < buf.len().saturating_sub(1) {
        let mut c = [0u8; 1];
        let n = do_read(&mut c);
        if n == 0 {
            do_yield();
            continue;
        }
        let b = c[0];
        if esc_state == 1 {
            if b == b'[' {
                esc_state = 2;
            } else {
                esc_state = 0;
            }
            continue;
        }
        if esc_state == 2 {
            if b == b'A' && history_len > 0 && *history_idx > 0 {
                *history_idx -= 1;
                let prev = &history[*history_idx];
                let mut len = 0;
                while len < LINE_MAX - 1 && prev[len] != 0 {
                    len += 1;
                }
                for j in 0..len {
                    buf[j] = prev[j];
                }
                i = len;
                buf[i] = 0;
                do_write("\r> ");
                do_write(core::str::from_utf8(&buf[..i]).unwrap_or(""));
            } else if b == b'B' && *history_idx < history_len {
                *history_idx += 1;
                if *history_idx < history_len {
                    let prev = &history[*history_idx];
                    let mut len = 0;
                    while len < LINE_MAX - 1 && prev[len] != 0 {
                        len += 1;
                    }
                    for j in 0..len {
                        buf[j] = prev[j];
                    }
                    i = len;
                } else {
                    i = 0;
                }
                buf[i] = 0;
                do_write("\r> ");
                do_write(core::str::from_utf8(&buf[..i]).unwrap_or(""));
            }
            esc_state = 0;
            continue;
        }
        if b == 0x1B {
            esc_state = 1;
            continue;
        }
        if b == 0x08 || b == 127 {
            if i > 0 {
                i -= 1;
                do_write("\x08 \x08");
            }
            continue;
        }
        if b == b'\n' || b == b'\r' {
            buf[i] = 0;
            return i;
        }
        buf[i] = b;
        do_write(core::str::from_utf8(&[b]).unwrap_or(""));
        i += 1;
    }
    buf[i] = 0;
    i
}

fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c == ' ' || c == '\t' || c == '\r' || c == '\n')
}

fn parse_args(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn parse_echo_args(line: &str) -> Option<EchoResult<'_>> {
    let rest = line.strip_prefix("echo")?;
    let rest = trim(rest);
    if let Some(idx) = rest.find('>') {
        let (left, right) = rest.split_at(idx);
        let content = trim(left).trim_matches('"');
        let path = trim(&right[1..]);
        if !path.is_empty() {
            return Some(EchoResult::ToFile { content, path });
        }
    }
    Some(EchoResult::ToStdout(trim(rest).trim_matches('"')))
}

enum EchoResult<'a> {
    ToStdout(&'a str),
    ToFile { content: &'a str, path: &'a str },
}

fn show_welcome_screen() {
    do_clear_screen();
    do_write("\r\n");
    do_write("  ==========================================\r\n");
    do_write("            T S - O S\r\n");
    do_write("  ==========================================\r\n");
    do_write("\r\n");
    do_write("  This is a living operating system powered by the\r\n");
    do_write("  Strongest Node Framework from BoggersTheCIG.\r\n");
    do_write("\r\n");
    do_write("  Basic commands:\r\n");
    do_write("    help   - show full command list\r\n");
    do_write("    ps     - list processes (nodes)\r\n");
    do_write("    spawn  - spawn new process\r\n");
    do_write("    echo   - echo text (or echo \"x\" > file)\r\n");
    do_write("    ls     - list directory\r\n");
    do_write("    cat    - read file\r\n");
    do_write("    touch  - create file\r\n");
    do_write("    mkdir  - create directory\r\n");
    do_write("    shutdown - checkpoint and halt\r\n");
    do_write("\r\n");
    do_write("  Type 'help' for full command list.\r\n");
    do_write("  Nodes emerge automatically based on system tension.\r\n");
    do_write("\r\n");
    do_write("  Press any key to continue, or wait 4 seconds...\r\n");
    do_write("\r\n");

    const WAIT_TICKS: u32 = 400;
    for _ in 0..WAIT_TICKS {
        if do_poll_key() {
            let mut discard = [0u8; 1];
            do_read(&mut discard);
            break;
        }
        do_yield();
    }

    do_clear_screen();
}

fn show_cmd_help(cmd: &str) -> bool {
    let (name, desc) = match cmd {
        "help" => ("help [cmd]", "Show command list or help for specific command"),
        "about" | "welcome" => ("about", "Strongest Node philosophy and status"),
        "ps" => ("ps", "List all processes (nodes) with activation/tension"),
        "spawn" => ("spawn", "Spawn a new process (node emerges)"),
        "echo" => ("echo TEXT [> FILE]", "Print TEXT or write to FILE"),
        "ls" => ("ls [PATH]", "List directory (default: cwd)"),
        "cat" => ("cat PATH", "Read and display file contents"),
        "touch" => ("touch PATH", "Create empty file"),
        "mkdir" => ("mkdir PATH", "Create directory"),
        "rm" => ("rm PATH", "Remove file or empty directory"),
        "cd" => ("cd [PATH]", "Change directory (default: /)"),
        "pwd" => ("pwd", "Print working directory"),
        "getpid" => ("getpid", "Print process ID"),
        "wait" => ("wait", "Wait for a child process to exit"),
        "kill" => ("kill PID", "Send SIGKILL to process"),
        "wc" => ("wc FILE", "Word/line/char count"),
        "head" => ("head FILE [N]", "First N lines (default 10)"),
        "tail" => ("tail FILE [N]", "Last N lines (default 10)"),
        "exit" => ("exit", "Exit current process"),
        "shutdown" => ("shutdown", "Checkpoint filesystem and halt"),
        _ => return false,
    };
    do_write(&alloc::format!("\r\n  {} - {}\r\n\r\n", name, desc));
    true
}

fn show_help() {
    do_write("\r\n  TS-OS Commands\r\n");
    do_write("  --------------\r\n");
    do_write("  help       Show this command list\r\n");
    do_write("  about      Strongest Node philosophy and status\r\n");
    do_write("  ps         List all processes (nodes) with activation/tension\r\n");
    do_write("  spawn      Spawn a new process (node emerges)\r\n");
    do_write("  echo TEXT  Print TEXT to screen\r\n");
    do_write("  echo \"TEXT\" > PATH  Write TEXT to file\r\n");
    do_write("  ls [PATH]  List directory (default: cwd)\r\n");
    do_write("  cat PATH   Read and display file contents\r\n");
    do_write("  touch PATH   Create empty file\r\n");
    do_write("  mkdir PATH   Create directory\r\n");
    do_write("  rm PATH    Remove file or empty directory\r\n");
    do_write("  cd [PATH]  Change directory (default: /)\r\n");
    do_write("  pwd        Print working directory\r\n");
    do_write("  getpid     Print process ID\r\n");
    do_write("  wait       Wait for a child process to exit\r\n");
    do_write("  kill PID   Send SIGKILL (9) to process\r\n");
    do_write("  wc [FILE]  Word/line/char count\r\n");
    do_write("  head [FILE] [N]  First N lines (default 10)\r\n");
    do_write("  tail [FILE] [N]  Last N lines (default 10)\r\n");
    do_write("  cmd1 | cmd2  Pipe output of cmd1 to input of cmd2\r\n");
    do_write("  cmd > FILE  Redirect stdout to file\r\n");
    do_write("  cmd < FILE  Redirect stdin from file\r\n");
    do_write("  exit       Exit current process\r\n");
    do_write("  shutdown   Checkpoint filesystem and halt\r\n");
    do_write("\r\n");
}

fn show_about() {
    do_write("\r\n  TS-OS - Strongest Node Operating System\r\n");
    do_write("  ----------------------------------------\r\n");
    do_write("  The kernel is the Strongest Node. All other components\r\n");
    do_write("  exist as secondary nodes that dynamically emerge,\r\n");
    do_write("  strengthen, spread activation, detect tension, decay,\r\n");
    do_write("  merge, or get pruned.\r\n");
    do_write("\r\n");
    do_write("  Philosophy (from BoggersTheCIG):\r\n");
    do_write("  - Strongest Node drives everything\r\n");
    do_write("  - Secondary nodes = processes\r\n");
    do_write("  - Emergence: nodes spawn when tension is high\r\n");
    do_write("  - Tension resolution: bugs and inefficiency are tensions\r\n");
    do_write("\r\n");
    do_write("  Current status: VGA + keyboard, Strongest Node scheduler,\r\n");
    do_write("  dynamic process spawning, in-RAM filesystem.\r\n");
    do_write("\r\n");
}

fn run_cmd(line: &str, cwd: &str, out_buf: &mut [u8], out_redirect: Option<&str>, in_redirect: Option<&str>) -> bool {
    let line = trim(line);
    if line.is_empty() {
        return false;
    }
    let args = parse_args(line);
    if args.is_empty() {
        return false;
    }
    let path_arg = |i: usize| -> String {
        args.get(i).map(|p| resolve_path(cwd, p)).unwrap_or_default()
    };
    match args[0] {
        "help" => {
            if let Some(cmd) = args.get(1) {
                if show_cmd_help(cmd) {
                } else {
                    do_write(&alloc::format!("  Unknown command: {}\r\n", cmd));
                }
            } else {
                show_help();
            }
            true
        }
        "about" | "welcome" => {
            show_about();
            true
        }
        "ps" => {
            let n = do_ps(out_buf);
            out_buf[n.min(out_buf.len().saturating_sub(1))] = 0;
            let s = core::str::from_utf8(&out_buf[..n]).unwrap_or("");
            if let Some(p) = out_redirect {
                do_write_file(&resolve_path(cwd, p), s.as_bytes());
            } else {
                do_write(s);
            }
            true
        }
        "echo" => {
            if let Some(result) = parse_echo_args(line) {
                match result {
                    EchoResult::ToStdout(content) => {
                        let s = alloc::format!("{}\r\n", content);
                        if let Some(p) = out_redirect {
                            do_write_file(&resolve_path(cwd, p), content.as_bytes());
                        } else {
                            do_write(&s);
                        }
                    }
                    EchoResult::ToFile { content, path } => {
                        do_write_file(&resolve_path(cwd, path), content.as_bytes());
                    }
                }
            }
            true
        }
        "spawn" => {
            let ret = do_spawn();
            if ret == !0u64 {
                do_write("spawn failed\r\n");
            } else {
                do_write("spawned\r\n");
            }
            true
        }
        "ls" => {
            let path = if args.len() > 1 { path_arg(1) } else { cwd.to_string() };
            let n = do_ls(&path, out_buf);
            out_buf[n.min(out_buf.len().saturating_sub(1))] = 0;
            let s = core::str::from_utf8(&out_buf[..n]).unwrap_or("");
            if let Some(p) = out_redirect {
                do_write_file(&resolve_path(cwd, p), s.as_bytes());
            } else {
                do_write(s);
            }
            true
        }
        "cat" => {
            let path = if let Some(p) = in_redirect {
                resolve_path(cwd, p)
            } else {
                path_arg(1)
            };
            if path.is_empty() {
                do_write("cat <path>\r\n");
            } else {
                let n = do_cat(&path, out_buf);
                out_buf[n.min(out_buf.len().saturating_sub(1))] = 0;
                let s = core::str::from_utf8(&out_buf[..n]).unwrap_or("");
                if let Some(p) = out_redirect {
                    do_write_file(&resolve_path(cwd, p), s.as_bytes());
                } else {
                    do_write(s);
                }
            }
            true
        }
        "touch" => {
            let path = path_arg(1);
            if path.is_empty() {
                do_write("touch <path>\r\n");
            } else if do_touch(&path) {
                do_write("ok\r\n");
            } else {
                do_write("touch failed\r\n");
            }
            true
        }
        "mkdir" => {
            let path = path_arg(1);
            if path.is_empty() {
                do_write("mkdir <path>\r\n");
            } else if do_mkdir(&path) {
                do_write("ok\r\n");
            } else {
                do_write("mkdir failed\r\n");
            }
            true
        }
        "rm" => {
            let path = path_arg(1);
            if path.is_empty() {
                do_write("rm <path>\r\n");
            } else if do_rm(&path) {
                do_write("ok\r\n");
            } else {
                do_write("rm failed (file not found or dir not empty)\r\n");
            }
            true
        }
        "pwd" => {
            let s = alloc::format!("{}\r\n", cwd);
            if let Some(p) = out_redirect {
                do_write_file(&resolve_path(cwd, p), cwd.as_bytes());
            } else {
                do_write(&s);
            }
            true
        }
        "getpid" => {
            let pid = do_getpid();
            if pid == !0u64 {
                do_write("getpid failed\r\n");
            } else {
                do_write(&alloc::format!("{}\r\n", pid));
            }
            true
        }
        "wait" => {
            let ret = do_wait();
            if ret == !0u64 {
                do_write("wait failed (no children or not a parent)\r\n");
            } else {
                let pid = (ret >> 8) & 0xFF;
                let status = ret & 0xFF;
                do_write(&alloc::format!("child {} exited with status {}\r\n", pid, status));
            }
            true
        }
        "kill" => {
            let pid_str = path_arg(1);
            if pid_str.is_empty() {
                do_write("kill <pid>\r\n");
            } else if let Ok(pid) = pid_str.parse::<u32>() {
                if do_kill(pid, 9) {
                    do_write("ok\r\n");
                } else {
                    do_write("kill failed\r\n");
                }
            } else {
                do_write("kill: invalid pid\r\n");
            }
            true
        }
        "wc" => {
            let path = if args.len() > 1 { path_arg(1) } else { String::new() };
            if path.is_empty() {
                do_write("wc <file>\r\n");
            } else {
                let n = do_cat(&path, out_buf);
                out_buf[n.min(out_buf.len().saturating_sub(1))] = 0;
                let s = core::str::from_utf8(&out_buf[..n]).unwrap_or("");
                let lines = s.lines().count();
                let words = s.split_whitespace().count();
                let chars = s.len();
                do_write(&alloc::format!("{}\t{}\t{}\t{}\r\n", lines, words, chars, path));
            }
            true
        }
        "head" => {
            let n: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(10);
            let path = if args.len() > 1 { path_arg(1) } else { String::new() };
            if path.is_empty() {
                do_write("head <file> [n]\r\n");
            } else {
                let cnt = do_cat(&path, out_buf);
                out_buf[cnt.min(out_buf.len().saturating_sub(1))] = 0;
                let s = core::str::from_utf8(&out_buf[..cnt]).unwrap_or("");
                for (i, line) in s.lines().enumerate() {
                    if i >= n {
                        break;
                    }
                    do_write(&alloc::format!("{}\r\n", line));
                }
            }
            true
        }
        "tail" => {
            let n: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(10);
            let path = if args.len() > 1 { path_arg(1) } else { String::new() };
            if path.is_empty() {
                do_write("tail <file> [n]\r\n");
            } else {
                let cnt = do_cat(&path, out_buf);
                out_buf[cnt.min(out_buf.len().saturating_sub(1))] = 0;
                let s = core::str::from_utf8(&out_buf[..cnt]).unwrap_or("");
                let lines: alloc::vec::Vec<&str> = s.lines().collect();
                let start = if lines.len() > n {
                    lines.len() - n
                } else {
                    0
                };
                for i in start..lines.len() {
                    do_write(&alloc::format!("{}\r\n", lines[i]));
                }
            }
            true
        }
        "exit" => {
            do_exit(0);
        }
        "shutdown" => {
            syscall0(SYS_SHUTDOWN);
            loop {
                core::hint::spin_loop();
            }
        }
        _ => {
            do_write("unknown: ");
            do_write(args[0]);
            do_write("\r\n");
            true
        }
    }
}

fn parse_redirects(line: &str) -> (&str, Option<&str>, Option<&str>) {
    let mut out: Option<&str> = None;
    let mut inp: Option<&str> = None;
    let mut cmd = trim(line);
    while let Some(idx_out) = cmd.find('>') {
        let (left, right) = cmd.split_at(idx_out);
        cmd = trim(left);
        let path = trim(&right[1..]).split_whitespace().next().unwrap_or("");
        if !path.is_empty() {
            out = Some(path);
        }
    }
    while let Some(idx_in) = cmd.find('<') {
        let (left, right) = cmd.split_at(idx_in);
        cmd = trim(left);
        let path = trim(&right[1..]).split_whitespace().next().unwrap_or("");
        if !path.is_empty() {
            inp = Some(path);
        }
    }
    (cmd, out, inp)
}

pub fn shell_main() {
    let mut line_buf = [0u8; LINE_MAX];
    let mut out_buf = [0u8; 512];
    let mut history: [[u8; LINE_MAX]; HISTORY_MAX] = [[0; LINE_MAX]; HISTORY_MAX];
    let mut history_len = 0usize;
    let mut history_idx = 0usize;
    let mut cwd = String::from("/");

    show_welcome_screen();

    loop {
        do_write("> ");
        let n = read_line(&mut line_buf, &history, history_len, &mut history_idx);
        let line = core::str::from_utf8(&line_buf[..n]).unwrap_or("");
        let line = trim(line);
        if line.is_empty() {
            do_yield();
            continue;
        }

        if !line.is_empty() {
            if history_len >= HISTORY_MAX {
                for i in 1..HISTORY_MAX {
                    history[i - 1] = history[i];
                }
                history_len = HISTORY_MAX - 1;
            }
            let h = &mut history[history_len];
            let copy = line.len().min(LINE_MAX - 1);
            for i in 0..copy {
                h[i] = line.as_bytes()[i];
            }
            h[copy] = 0;
            history_len += 1;
        }

        if let Some(pipe_idx) = line.find('|') {
            let left = trim(&line[..pipe_idx]);
            let right = trim(&line[pipe_idx + 1..]);
            if !left.is_empty() && !right.is_empty() {
                let mut pipe_buf = [0u8; 512];
                let n_left = if left.starts_with("ls ") {
                    let path = parse_args(left).get(1).map(|p| resolve_path(&cwd, p)).unwrap_or_else(|| cwd.clone());
                    do_ls(&path, &mut pipe_buf)
                } else if left.starts_with("cat ") {
                    let path = parse_args(left).get(1).map(|p| resolve_path(&cwd, p)).unwrap_or_default();
                    do_cat(&path, &mut pipe_buf)
                } else if left == "ps" {
                    do_ps(&mut pipe_buf)
                } else if left.starts_with("echo ") {
                    let content = left.strip_prefix("echo").map(trim).unwrap_or("").trim_matches('"');
                    let len = content.len().min(511);
                    for i in 0..len {
                        pipe_buf[i] = content.as_bytes()[i];
                    }
                    len
                } else {
                    do_write("pipe: left cmd not supported (ls, cat, ps, echo)\r\n");
                    do_yield();
                    continue;
                };
                pipe_buf[n_left.min(511)] = 0;
                let right_args = parse_args(right);
                if right_args.get(0) == Some(&"cat") {
                    if let Some(path) = right_args.get(1) {
                        let abs_path = resolve_path(&cwd, path);
                        do_write_file(&abs_path, &pipe_buf[..n_left]);
                    } else {
                        do_write(core::str::from_utf8(&pipe_buf[..n_left]).unwrap_or(""));
                    }
                } else {
                    do_write("pipe: right must be 'cat [file]' (e.g. ls | cat out.txt)\r\n");
                }
            }
        } else {
            let (cmd_part, out_redir, in_redir) = parse_redirects(line);
            let args = parse_args(cmd_part);
            if args.get(0) == Some(&"cd") {
                let path = if args.len() > 1 {
                    resolve_path(&cwd, args[1])
                } else {
                    "/".to_string()
                };
                if do_chdir(&path) {
                    cwd = path;
                } else {
                    do_write("cd failed\r\n");
                }
            } else {
                let _ = run_cmd(cmd_part, &cwd, &mut out_buf, out_redir, in_redir);
            }
        }
        do_yield();
    }
}
