/*
 * BTFOS (BoggersTheFish OS) - Kernel
 * GOAT-TS inspired: graph-driven kernel with cognition loop, activation/decay,
 * forces, curiosity, reflection, goal-generation, self-assessment.
 * C kernel, 32-bit protected mode, Multiboot. MIT License.
 */

#include "../include/btfos_config.h"
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#define BTFOS_ASSERT(c) do { if (!(c)) btfos_panic("assert"); } while(0)

/* ---- Multiboot ---- */
#define MULTIBOOT_MAGIC 0x2BADB002

typedef struct {
    uint32_t flags;
    uint32_t mem_lower;
    uint32_t mem_upper;
    uint32_t boot_device;
    uint32_t cmdline;
    uint32_t mods_count;
    uint32_t mods_addr;
    uint32_t syms[4];
    uint32_t mmap_length;
    uint32_t mmap_addr;
} __attribute__((packed)) multiboot_info_t;

/* ---- Port I/O ---- */
static inline void outb(uint16_t port, uint8_t v) {
    __asm__ volatile ("outb %0, %1" : : "a"(v), "Nd"(port));
}
static inline uint8_t inb(uint16_t port) {
    uint8_t v;
    __asm__ volatile ("inb %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}

/* ---- Serial (COM1) for logging ---- */
#define SERIAL_PORT 0x3F8
static void serial_putc(char c) {
    outb(SERIAL_PORT, c);
}
static void serial_puts(const char *s) {
    while (*s) serial_putc(*s++);
}
static void serial_init(void) {
    outb(SERIAL_PORT + 1, 0);
    outb(SERIAL_PORT + 3, 0x80);
    outb(SERIAL_PORT + 0, 0x03);
    outb(SERIAL_PORT + 1, 0);
    outb(SERIAL_PORT + 3, 0x03);
    outb(SERIAL_PORT + 2, 0xC7);
    outb(SERIAL_PORT + 4, 0x0B);
}

/* ---- VGA text buffer ---- */
#define VGA_BASE 0xB8000
#define VGA_W 80
#define VGA_H 25
static volatile uint16_t *vga = (volatile uint16_t *)VGA_BASE;
static int vga_x, vga_y;
static const uint8_t VGA_COLOR = 0x0A; /* green on black */

static void vga_clear(void) {
    for (int i = 0; i < VGA_W * VGA_H; i++) vga[i] = (uint16_t)((VGA_COLOR << 8) | ' ');
    vga_x = vga_y = 0;
}
static void vga_putc(char c) {
    if (c == '\n') { vga_x = 0; if (++vga_y >= VGA_H) vga_y = 0; return; }
    if (vga_x < VGA_W && vga_y < VGA_H)
        vga[vga_y * VGA_W + vga_x] = (uint16_t)((VGA_COLOR << 8) | (unsigned char)c);
    if (++vga_x >= VGA_W) { vga_x = 0; if (++vga_y >= VGA_H) vga_y = 0; }
}
static void vga_puts(const char *s) {
    while (*s) vga_putc(*s++);
}

/* ---- Simple decimal print (no stdio) ---- */
static void u32_to_str(uint32_t n, char *buf, int *len) {
    if (n == 0) { buf[(*len)++] = '0'; return; }
    char t[12]; int i = 0;
    while (n) { t[i++] = (char)('0' + n % 10); n /= 10; }
    while (i--) buf[(*len)++] = t[i];
}
static void log_str(const char *s) { serial_puts(s); vga_puts(s); }
static void log_u32(uint32_t n) {
    char b[12]; int l = 0;
    u32_to_str(n, b, &l);
    b[l] = '\0';
    log_str(b);
}
static void btfos_panic(const char *msg) {
    log_str("PANIC: "); log_str(msg); log_str("\n");
    for (;;) __asm__ volatile ("hlt");
}

/* ==================== GOAT-TS Graph ==================== */
typedef enum { NODE_PROCESS, NODE_PAGE, NODE_FILE, NODE_SYSCALL, NODE_GOAL } node_type_t;
typedef struct graph_node {
    uint32_t id;
    node_type_t type;
    float activation;   /* 0..1, decay over time */
    float tension;      /* for reasoning */
    uint32_t shard_id;  /* which shard (core) owns this */
    struct graph_node *next;
} graph_node_t;

typedef struct graph_edge {
    struct graph_node *from;
    struct graph_node *to;
    float weight;       /* dependency/affinity strength */
    struct graph_edge *next;
} graph_edge_t;

static graph_node_t  nodes[BTFOS_MAX_GRAPH_NODES];
static graph_edge_t  edges[BTFOS_MAX_EDGES];
static uint32_t      node_count, edge_count;

static graph_node_t *graph_add_node(node_type_t type, uint32_t shard) {
    BTFOS_ASSERT(node_count < BTFOS_MAX_GRAPH_NODES);
    graph_node_t *n = &nodes[node_count++];
    n->id = node_count - 1;
    n->type = type;
    n->activation = 0.5f;
    n->tension = 0.0f;
    n->shard_id = shard % BTFOS_SHARD_COUNT;
    n->next = NULL;
    return n;
}
static void graph_add_edge(graph_node_t *from, graph_node_t *to, float w) {
    BTFOS_ASSERT(edge_count < BTFOS_MAX_EDGES);
    graph_edge_t *e = &edges[edge_count++];
    e->from = from;
    e->to = to;
    e->weight = w;
    e->next = NULL;
}
/* Activation spread: boost activation of nodes connected to high-activation nodes */
static void graph_spread_activation(void) {
    for (uint32_t i = 0; i < edge_count; i++) {
        graph_edge_t *e = &edges[i];
        float add = e->from->activation * e->weight * 0.1f;
        if (e->to->activation + add <= 1.0f) e->to->activation += add;
    }
}
/* Decay: reduce activation over time */
static void graph_decay(float rate) {
    for (uint32_t i = 0; i < node_count; i++)
        if (nodes[i].activation > 0.05f) nodes[i].activation -= rate;
}

/* ==================== Memory (graph-based pages) ==================== */
typedef enum { PAGE_ACTIVE, PAGE_DORMANT, PAGE_DEEP } page_state_t;
typedef struct page_node {
    graph_node_t *graph_node;  /* link into main graph */
    uint32_t pfn;              /* page frame number */
    page_state_t state;
} page_node_t;

static page_node_t page_nodes[BTFOS_MAX_PAGES];
static uint32_t page_alloc_count;

static void memory_init(void) {
    for (uint32_t i = 0; i < BTFOS_MAX_PAGES; i++) {
        page_node_t *p = &page_nodes[i];
        p->pfn = i;
        p->state = PAGE_ACTIVE;
        p->graph_node = graph_add_node(NODE_PAGE, i % BTFOS_SHARD_COUNT);
        p->graph_node->activation = 0.3f;
    }
    page_alloc_count = 0;
}
static void *memory_alloc(void) {
    for (uint32_t i = 0; i < BTFOS_MAX_PAGES; i++) {
        if (page_nodes[i].state != PAGE_ACTIVE) continue;
        page_nodes[i].state = PAGE_ACTIVE;
        page_nodes[i].graph_node->activation = 0.8f;
        page_alloc_count++;
        return (void *)(uintptr_t)(page_nodes[i].pfn * 4096);
    }
    return NULL; /* OOM */
}
static void memory_free(void *ptr) {
    (void)ptr;
    if (page_alloc_count > 0) page_alloc_count--;
}

/* ==================== Process table (GOAT-TS: nodes = processes) ==================== */
typedef enum { PROC_READY, PROC_RUN, PROC_BLOCKED } proc_state_t;
typedef struct process {
    uint32_t pid;
    proc_state_t state;
    graph_node_t *graph_node;
    float affinity_cpu;   /* force: preferred "cpu" (shard) */
    uint32_t ticks;
} process_t;

static process_t processes[BTFOS_MAX_PROCESSES];
static uint32_t num_procs;
static uint32_t current_pid;   /* currently running */
static uint32_t next_pid;

static void process_init(void) {
    num_procs = 0;
    next_pid = 1;
    current_pid = 0;
    for (int i = 0; i < BTFOS_MAX_PROCESSES; i++)
        processes[i].pid = 0;
}
static uint32_t process_fork(void) {
    BTFOS_ASSERT(num_procs < BTFOS_MAX_PROCESSES);
    uint32_t pid = next_pid++;
    process_t *p = &processes[num_procs++];
    p->pid = pid;
    p->state = PROC_READY;
    p->graph_node = graph_add_node(NODE_PROCESS, pid % BTFOS_SHARD_COUNT);
    p->graph_node->activation = 0.6f;
    p->affinity_cpu = (float)(pid % BTFOS_SHARD_COUNT);
    p->ticks = 0;
    return pid;
}
static process_t *process_get(uint32_t pid) {
    for (uint32_t i = 0; i < num_procs; i++)
        if (processes[i].pid == pid) return &processes[i];
    return NULL;
}

/* ==================== Cognition loop (scheduler tick) ==================== */
static uint32_t tick_count;
static float global_tension;   /* for reflection: bottleneck detection */

static void cognition_loop(void) {
    /* Mark current runner as READY so it can be rescheduled */
    process_t *cur = process_get(current_pid);
    if (cur) cur->state = PROC_READY;
    /* Stage: activation spread */
    graph_spread_activation();
    /* Stage: decay */
    graph_decay(0.01f);
#if BTFOS_ENABLE_FORCES
    /* Forces: affinity (e.g. CPU pinning) - prefer running on same shard */
    for (uint32_t i = 0; i < num_procs; i++) {
        process_t *p = &processes[i];
        if (p->pid == 0) continue;
        uint32_t shard = (uint32_t)p->affinity_cpu % BTFOS_SHARD_COUNT;
        if (p->graph_node->shard_id != shard)
            p->graph_node->activation += 0.02f; /* slight boost for affinity */
    }
#endif
#if BTFOS_ENABLE_REFLECTION
    /* Reflection: compute global tension (bottleneck proxy) */
    global_tension = 0.0f;
    for (uint32_t i = 0; i < node_count; i++)
        global_tension += nodes[i].tension;
    if (node_count) global_tension /= (float)node_count;
    if (global_tension > 0.5f) {
        /* Log hypothesis for optimization */
        log_str("{\"reflection\":\"tension_high\",\"hypothesis\":\"Optimize scheduler if tension >0.5\"}\n");
    }
#endif
#if BTFOS_ENABLE_CURIOSITY
    /* Curiosity: boost activation of idle/low-activation processes for reuse */
    for (uint32_t i = 0; i < num_procs; i++) {
        if (processes[i].pid == 0) continue;
        if (processes[i].graph_node->activation < 0.3f)
            processes[i].graph_node->activation += 0.05f;
    }
#endif
    /* Schedule: pick process with highest activation that is READY */
    uint32_t best = 0;
    float best_act = -1.0f;
    for (uint32_t i = 0; i < num_procs; i++) {
        if (processes[i].pid == 0 || processes[i].state != PROC_READY) continue;
        float a = processes[i].graph_node->activation;
        if (a > best_act) { best_act = a; best = processes[i].pid; }
    }
    if (best_act >= 0) current_pid = best;
    process_t *cur = process_get(current_pid);
    if (cur) { cur->state = PROC_RUN; cur->ticks++; }
    tick_count++;
}

/* ==================== System call ingestion (provenance graph) ==================== */
typedef struct {
    uint32_t pid;
    const char *name;
    uint32_t arg0, arg1;
    uint32_t tick;
} syscall_log_t;

static syscall_log_t syscall_log[BTFOS_MAX_SYSCALL_LOG];
static uint32_t syscall_log_head;

static void syscall_ingest(uint32_t pid, const char *name, uint32_t a0, uint32_t a1) {
    if (syscall_log_head >= BTFOS_MAX_SYSCALL_LOG) syscall_log_head = 0;
    syscall_log_t *e = &syscall_log[syscall_log_head++];
    e->pid = pid;
    e->name = name;
    e->arg0 = a0;
    e->arg1 = a1;
    e->tick = tick_count;
    /* Add as graph node for provenance */
    graph_node_t *n = graph_add_node(NODE_SYSCALL, pid % BTFOS_SHARD_COUNT);
    n->activation = 0.7f;
    process_t *p = process_get(pid);
    if (p && p->graph_node) graph_add_edge(p->graph_node, n, 1.0f);
}

/* Stub syscalls (no real fork/exec; simulate for graph) */
static uint32_t sys_fork(void) {
    uint32_t pid = process_fork();
    syscall_ingest(pid, "fork", 0, 0);
    return pid;
}
static void sys_exec(const char *path) {
    (void)path;
    syscall_ingest(current_pid, "exec", (uint32_t)(uintptr_t)path, 0);
}
static int sys_read(int fd, void *buf, size_t n) {
    syscall_ingest(current_pid, "read", (uint32_t)fd, (uint32_t)n);
    (void)buf;
    return (int)n;
}
static int sys_write(int fd, const void *buf, size_t n) {
    syscall_ingest(current_pid, "write", (uint32_t)fd, (uint32_t)n);
    (void)buf;
    return (int)n;
}
static int sys_open(const char *path, int flags) {
    syscall_ingest(current_pid, "open", (uint32_t)(uintptr_t)path, (uint32_t)flags);
    return 0;
}
static void sys_close(int fd) {
    syscall_ingest(current_pid, "close", (uint32_t)fd, 0);
}
static void sys_exit(int code) {
    syscall_ingest(current_pid, "exit", (uint32_t)code, 0);
    (void)code;
}
static uint32_t sys_getpid(void) {
    return current_pid;
}
static void sys_yield(void) {
    syscall_ingest(current_pid, "yield", 0, 0);
    cognition_loop();
}
static uint32_t sys_gettime(void) {
    syscall_ingest(current_pid, "gettime", 0, 0);
    return tick_count;
}

/* ==================== In-memory FS (triples) ==================== */
typedef struct {
    char subj[32];
    char pred[32];
    char obj[64];
} triple_t;

static triple_t triples[BTFOS_MAX_TRIPLES];
static uint32_t triple_count;

static void fs_init(void) {
    triple_count = 0;
    /* Ingest root as text triple */
    triple_t *t = &triples[triple_count++];
    const char *s = "root", *p = "type", *o = "dir";
    for (int i = 0; i < 31 && s[i]; i++) t->subj[i] = s[i];
    for (int i = 0; i < 31 && p[i]; i++) t->pred[i] = p[i];
    for (int i = 0; i < 63 && o[i]; i++) t->obj[i] = o[i];
    /* /boot, /tmp as dirs */
    const char *dirs[][3] = { {"boot", "type", "dir"}, {"tmp", "type", "dir"} };
    for (int d = 0; d < 2 && triple_count < BTFOS_MAX_TRIPLES; d++) {
        triple_t *tr = &triples[triple_count++];
        for (int i = 0; i < 31 && dirs[d][0][i]; i++) tr->subj[i] = dirs[d][0][i];
        for (int i = 0; i < 31 && dirs[d][1][i]; i++) tr->pred[i] = dirs[d][1][i];
        for (int i = 0; i < 63 && dirs[d][2][i]; i++) tr->obj[i] = dirs[d][2][i];
    }
}
static void fs_mkdir(const char *path) {
    if (triple_count >= BTFOS_MAX_TRIPLES) return;
    triple_t *t = &triples[triple_count++];
    int i;
    for (i = 0; i < 31 && path[i]; i++) t->subj[i] = path[i]; t->subj[i] = '\0';
    const char *p = "type", *o = "dir";
    for (i = 0; i < 31 && p[i]; i++) t->pred[i] = p[i]; t->pred[i] = '\0';
    for (i = 0; i < 63 && o[i]; i++) t->obj[i] = o[i]; t->obj[i] = '\0';
}
static void fs_stat(const char *path) {
    const char *type = fs_lookup(path, "type");
    if (type) { log_str("stat "); log_str(path); log_str(" type="); log_str(type); log_str("\n"); }
}
static void fs_ingest_file(const char *path, const char *content) {
    if (triple_count >= BTFOS_MAX_TRIPLES - 2) return;
    triple_t *t1 = &triples[triple_count++];
    triple_t *t2 = &triples[triple_count++];
    const char *p = "path", *t = "type", *f = "file";
    int i;
    for (i = 0; i < 31 && path[i]; i++) t1->subj[i] = path[i]; t1->subj[i] = '\0';
    for (i = 0; i < 31 && p[i]; i++) t1->pred[i] = p[i]; t1->pred[i] = '\0';
    for (i = 0; i < 63 && path[i]; i++) t2->subj[i] = path[i]; t2->subj[i] = '\0';
    for (i = 0; i < 31 && t[i]; i++) t2->pred[i] = t[i]; t2->pred[i] = '\0';
    for (i = 0; i < 63 && f[i]; i++) t2->obj[i] = f[i]; t2->obj[i] = '\0';
    (void)content;
}
static const char *fs_lookup(const char *subj, const char *pred) {
    for (uint32_t i = 0; i < triple_count; i++) {
        int ok = 1;
        for (int j = 0; subj[j] || triples[i].subj[j]; j++)
            if (subj[j] != triples[i].subj[j]) { ok = 0; break; }
        for (int j = 0; pred[j] || triples[i].pred[j]; j++)
            if (pred[j] != triples[i].pred[j]) { ok = 0; break; }
        if (ok) return triples[i].obj;
    }
    return NULL;
}

/* ==================== Self-assessment (scan kernel as text, hypotheses) ==================== */
static const char *kernel_source_tag = BTFOS_KERNEL_SOURCE_TAG;
static void self_assess_scan(void) {
    /* "Scan" kernel: we use a tag string as proxy for kernel identity */
    log_str("{\"self_assess\":\"scan\",\"tag\":\"");
    log_str(kernel_source_tag);
    log_str("\"}\n");
    if (global_tension > 0.5f) {
        log_str("{\"hypothesis\":\"Optimize scheduler if tension >0.5\"}\n");
    }
}

/* ==================== Plugins (loadable modules from dir) ==================== */
#define MAX_PLUGINS 8
static const char *plugin_names[MAX_PLUGINS];
static uint32_t plugin_count;

static void plugins_discover(void) {
    plugin_count = 0;
    /* Stub: in real impl we'd read directory; here we register placeholder */
    if (plugin_count < MAX_PLUGINS)
        plugin_names[plugin_count++] = "driver_vga";
    if (plugin_count < MAX_PLUGINS)
        plugin_names[plugin_count++] = "driver_serial";
}
static void plugins_log(void) {
    log_str("{\"plugins\":[");
    for (uint32_t i = 0; i < plugin_count; i++) {
        if (i) log_str(",");
        log_str("\""); log_str(plugin_names[i]); log_str("\"");
    }
    log_str("]}\n");
}

/* ==================== Shell (ingest commands, reason outputs) ==================== */
static char shell_buf[BTFOS_SHELL_BUF];
static int shell_len;

static void shell_ingest_command(const char *cmd) {
    /* Ingest as triple: (shell, last_cmd, cmd) */
    if (triple_count >= BTFOS_MAX_TRIPLES) return;
    triple_t *t = &triples[triple_count++];
    const char *s = "shell", *p = "last_cmd";
    int i;
    for (i = 0; i < 31 && s[i]; i++) t->subj[i] = s[i]; t->subj[i] = '\0';
    for (i = 0; i < 31 && p[i]; i++) t->pred[i] = p[i]; t->pred[i] = '\0';
    for (i = 0; i < 63 && cmd[i]; i++) t->obj[i] = cmd[i];
    t->obj[i < 63 ? i : 63] = '\0';
}
static void shell_reason_output(const char *cmd) {
    if (cmd[0] == 'h' && cmd[1] == 'e' && cmd[2] == 'l' && cmd[3] == 'p') {
        log_str("help: help | status | run | exit | bench | stat <path>\n");
        return;
    }
    if (cmd[0] == 's' && cmd[1] == 't' && cmd[2] == 'a' && cmd[3] == 't' && cmd[4] == 'u' && cmd[5] == 's') {
        log_str("tick="); log_u32(tick_count);
        log_str(" procs="); log_u32(num_procs);
        log_str(" tension="); log_str(global_tension > 0.5f ? "high" : "ok");
        log_str("\n");
        return;
    }
    if (cmd[0] == 'r' && cmd[1] == 'u' && cmd[2] == 'n') {
        uint32_t pid = sys_fork();
        log_str("forked pid="); log_u32(pid); log_str("\n");
        return;
    }
    if (cmd[0] == 'e' && cmd[1] == 'x' && cmd[2] == 'i' && cmd[3] == 't') {
        log_str("bye.\n");
        return;
    }
    if (cmd[0] == 'b' && cmd[1] == 'e' && cmd[2] == 'n' && cmd[3] == 'c' && cmd[4] == 'h') {
        log_str("time="); log_u32(sys_gettime()); log_str("\n");
        return;
    }
    if (cmd[0] == 's' && cmd[1] == 't' && cmd[2] == 'a' && cmd[3] == 't' && cmd[4] == ' ') {
        fs_stat((const char *)&cmd[5]);
        return;
    }
    log_str("unknown cmd. try help\n");
}
static void shell_step(void) {
    /* Simulate one command for demo */
    const char *cmd = "status";
    shell_ingest_command(cmd);
    shell_reason_output(cmd);
}

/* ==================== Monitoring (JSON stats) ==================== */
static void monitor_export(void) {
    log_str("{\"tick\":");
    log_u32(tick_count);
    log_str(",\"procs\":");
    log_u32(num_procs);
    log_str(",\"nodes\":");
    log_u32(node_count);
    log_str(",\"tension\":");
    log_str(global_tension > 0.5f ? "1" : "0");
    log_str("}\n");
}

/* ==================== Kernel entry ==================== */
void kernel_main(uint32_t magic, uint32_t mb_info_phys) {
    (void)mb_info_phys;
    serial_init();
    vga_clear();
    if (magic != MULTIBOOT_MAGIC) {
        log_str("Invalid multiboot magic\n");
        btfos_panic("multiboot");
    }
    log_str("BTFOS boot...\n");
    node_count = 0;
    edge_count = 0;
    memory_init();
    process_init();
    fs_init();
    tick_count = 0;
    global_tension = 0.0f;
    syscall_log_head = 0;
    plugins_discover();
    /* Bootstrap one process */
    process_fork();
    current_pid = 1;
    process_get(1)->state = PROC_RUN;
    log_str("BTFOS Ready\n");
    /* Run cognition loop a few times (skip in benchmark mode; we run below) */
#if !BTFOS_BENCHMARK_MODE
    for (int i = 0; i < BTFOS_TICKS_PER_LOOP; i++)
        cognition_loop();
#endif
    monitor_export();
    self_assess_scan();
    plugins_log();
    /* Shell: one demo command */
    shell_step();
    log_str("BTFOS shell ok.\n");
#if BTFOS_BENCHMARK_MODE
    /* Benchmark: run BTFOS_TICKS_PER_LOOP cognition ticks then report */
    for (uint32_t i = 0; i < (uint32_t)BTFOS_TICKS_PER_LOOP; i++)
        cognition_loop();
    log_str("{\"benchmark_ticks\":");
    log_u32(tick_count);
    log_str(",\"benchmark_done\":1}\n");
    log_str("BTFOS benchmark done.\n");
#else
    for (;;) {
        cognition_loop();
        if (tick_count > 1000) break; /* exit demo after 1000 ticks */
    }
#endif
    log_str("BTFOS halt.\n");
    for (;;) __asm__ volatile ("hlt");
}
