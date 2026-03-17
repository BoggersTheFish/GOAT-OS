//! Minimal shell (runs in user mode via node 0)

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

fn do_exit(status: u64) -> ! {
    unsafe {
        core::arch::asm!("int 0x80", in("rax") SYS_EXIT, in("rdi") status, options(nostack, preserves_flags));
    }
    loop {
        core::hint::spin_loop();
    }
}

fn read_line(buf: &mut [u8]) -> usize {
    let mut i = 0;
    while i < buf.len().saturating_sub(1) {
        let n = do_read(&mut buf[i..i + 1]);
        if n == 0 {
            do_yield();
            continue;
        }
        if buf[i] == b'\n' || buf[i] == b'\r' {
            buf[i] = 0;
            return i;
        }
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

fn show_help() {
    do_write("\r\n  TS-OS Commands\r\n");
    do_write("  --------------\r\n");
    do_write("  help       Show this command list\r\n");
    do_write("  about      Strongest Node philosophy and status\r\n");
    do_write("  ps         List all processes (nodes) with activation/tension\r\n");
    do_write("  spawn      Spawn a new process (node emerges)\r\n");
    do_write("  echo TEXT  Print TEXT to screen\r\n");
    do_write("  echo \"TEXT\" > PATH  Write TEXT to file\r\n");
    do_write("  ls [PATH]  List directory (default: /)\r\n");
    do_write("  cat PATH   Read and display file contents\r\n");
    do_write("  touch PATH   Create empty file\r\n");
    do_write("  mkdir PATH   Create directory\r\n");
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

pub fn shell_main() {
    let mut line_buf = [0u8; 128];
    let mut out_buf = [0u8; 256];

    show_welcome_screen();

    loop {
        do_write("> ");
        let n = read_line(&mut line_buf);
        let line = core::str::from_utf8(&line_buf[..n]).unwrap_or("");
        let line = trim(line);
        if line.is_empty() {
            continue;
        }

        let args = parse_args(line);

        if args.is_empty() {
            continue;
        }

        match args[0] {
            "help" => show_help(),
            "about" | "welcome" => show_about(),
            "ps" => {
                let n = do_ps(&mut out_buf);
                out_buf[n.min(out_buf.len() - 1)] = 0;
                do_write(core::str::from_utf8(&out_buf[..n]).unwrap_or(""));
            }
            "echo" => {
                if let Some(result) = parse_echo_args(line) {
                    match result {
                        EchoResult::ToStdout(content) => {
                            let s = alloc::format!("{}\r\n", content);
                            do_write(&s);
                        }
                        EchoResult::ToFile { content, path } => {
                            do_write_file(path, content.as_bytes());
                        }
                    }
                }
            }
            "spawn" => {
                let ret = do_spawn();
                if ret == !0u64 {
                    do_write("spawn failed\r\n");
                } else {
                    do_write("spawned\r\n");
                }
            }
            "ls" => {
                let path = args.get(1).copied().unwrap_or("/");
                let n = do_ls(path, &mut out_buf);
                out_buf[n.min(out_buf.len() - 1)] = 0;
                do_write(core::str::from_utf8(&out_buf[..n]).unwrap_or(""));
            }
            "cat" => {
                let path = args.get(1).copied().unwrap_or("");
                if path.is_empty() {
                    do_write("cat <path>\r\n");
                } else {
                    let n = do_cat(path, &mut out_buf);
                    out_buf[n.min(out_buf.len() - 1)] = 0;
                    do_write(core::str::from_utf8(&out_buf[..n]).unwrap_or(""));
                }
            }
            "touch" => {
                let path = args.get(1).copied().unwrap_or("");
                if path.is_empty() {
                    do_write("touch <path>\r\n");
                } else if do_touch(path) {
                    do_write("ok\r\n");
                } else {
                    do_write("touch failed\r\n");
                }
            }
            "mkdir" => {
                let path = args.get(1).copied().unwrap_or("");
                if path.is_empty() {
                    do_write("mkdir <path>\r\n");
                } else if do_mkdir(path) {
                    do_write("ok\r\n");
                } else {
                    do_write("mkdir failed\r\n");
                }
            }
            "exit" => do_exit(0),
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
            }
        }
        do_yield();
    }
}
