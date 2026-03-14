/*
 * BTFOS Kernel - Graph-Driven Cognition OS
 * Copyright (c) Ben Michalek (BoggersTheFish). MIT License.
 * Single-file kernel: compiles with gcc -m32 -I include -ffreestanding -nostdlib.
 * Boot entry: kernel_main(uint32_t magic, uint32_t mb_info) from boot.asm.
 */

#include <stdint.h>
#include <stddef.h>
#include "btfos_config.h"

/* ----- Port I/O ----- */
static inline void outb(uint16_t port, uint8_t v) {
    __asm__ volatile ("outb %0, %1" : : "a"(v), "Nd"(port));
}
static inline uint8_t inb(uint16_t port) {
    uint8_t v;
    __asm__ volatile ("inb %1, %0" : "=a"(v) : "Nd"(port));
    return v;
}

/* ----- Serial (COM1) ----- */
#define SERIAL_PORT 0x3F8
static void serial_putc(char c) { outb(SERIAL_PORT, c); }
static void serial_puts(const char *s) { while (*s) serial_putc(*s++); }

static void serial_init(void) {
    outb(SERIAL_PORT + 1, 0);
    outb(SERIAL_PORT + 3, 0x80);
    outb(SERIAL_PORT + 0, 0x03);
    outb(SERIAL_PORT + 1, 0);
    outb(SERIAL_PORT + 3, 0x03);
    outb(SERIAL_PORT + 2, 0xC7);
    outb(SERIAL_PORT + 4, 0x0B);
}

/* ----- VGA ----- */
#define VGA_BASE 0xB8000
#define VGA_W 80
#define VGA_H 25
static volatile uint16_t *vga = (volatile uint16_t *)VGA_BASE;
static int vga_x, vga_y;
static const uint8_t VGA_COLOR = 0x0A;

static void vga_clear(void) {
    for (int i = 0; i < VGA_W * VGA_H; i++)
        vga[i] = (uint16_t)((VGA_COLOR << 8) | ' ');
    vga_x = 0;
    vga_y = 0;
}
static void vga_putc(char c) {
    if (c == '\n') {
        vga_x = 0;
        if (++vga_y >= VGA_H)
            vga_y = 0;
        return;
    }
    if (vga_x < VGA_W && vga_y < VGA_H)
        vga[vga_y * VGA_W + vga_x] = (uint16_t)((VGA_COLOR << 8) | (unsigned char)c);
    if (++vga_x >= VGA_W) {
        vga_x = 0;
        if (++vga_y >= VGA_H)
            vga_y = 0;
    }
}
static void vga_puts(const char *s) { while (*s) vga_putc(*s++); }

static void monitor_print(const char *s) {
    serial_puts(s);
    vga_puts(s);
}

/* ----- Memory (stub) ----- */
void memory_init(void) { (void)0; }

/* ----- Graph (GOAT-TS style) ----- */
typedef struct graph_node_inner {
    uint32_t pid;
    float activation;
    int state;
    struct graph_node_inner *next;
} graph_node_inner_t;

typedef struct graph_t {
    graph_node_inner_t *nodes;
    graph_node_inner_t *last;
} graph_t;

static graph_node_inner_t graph_node_pool[BTFOS_MAX_GRAPH_NODES];
static unsigned int graph_pool_idx;

static graph_t *graph_create(void) {
    static graph_t g;
    g.nodes = NULL;
    g.last = NULL;
    graph_pool_idx = 0;
    return &g;
}

static void graph_add_node(graph_t *g, uint32_t pid, float act, int state) {
    if (graph_pool_idx >= BTFOS_MAX_GRAPH_NODES)
        return;
    graph_node_inner_t *n = &graph_node_pool[graph_pool_idx++];
    n->pid = pid;
    n->activation = act;
    n->state = state;
    n->next = NULL;
    if (g->last)
        g->last->next = n;
    else
        g->nodes = n;
    g->last = n;
}

static void graph_add_edge(graph_t *g, uint32_t from, uint32_t to, const char *kind) {
    (void)g;
    (void)from;
    (void)to;
    (void)kind;
}

static void graph_spread_activation(graph_t *g, void *seeds, float decay) {
    (void)seeds;
    (void)decay;
    graph_node_inner_t *n;
    for (n = g->nodes; n; n = n->next) {
        if (n->activation > 0.05f)
            n->activation *= 0.9f;
    }
}

static void graph_decay_states(graph_t *g) {
    graph_node_inner_t *n;
    for (n = g->nodes; n; n = n->next)
        (void)n;
}

static void graph_apply_forces(graph_t *g) {
    (void)g;
}

static void graph_curiosity(graph_t *g) {
    (void)g;
}

static void graph_reflection(graph_t *g) {
    (void)g;
}

static void graph_goal_generator(graph_t *g) {
    (void)g;
}

static void graph_sandbox_test(graph_t *g) {
    (void)g;
}

static float graph_get_activation(graph_t *g, uint32_t pid) {
    graph_node_inner_t *n;
    for (n = g->nodes; n; n = n->next) {
        if (n->pid == pid)
            return n->activation;
    }
    return 0.5f;
}

static int graph_get_state(graph_t *g, uint32_t pid) {
    graph_node_inner_t *n;
    for (n = g->nodes; n; n = n->next) {
        if (n->pid == pid)
            return n->state;
    }
    return 0;
}

static void graph_destroy(graph_t *g) {
    (void)g;
}

/* ----- Process ----- */
typedef struct process process_t;
struct process {
    uint32_t pid;
    float activation;
    int state;
    process_t *parent;
    process_t *next;
};

static process_t process_pool[BTFOS_MAX_PROCESSES];
static unsigned int process_count;

void process_init(void) {
    process_count = 0;
    for (unsigned int i = 0; i < BTFOS_MAX_PROCESSES; i++)
        process_pool[i].pid = 0;
}

process_t *process_list(void) {
    if (process_count == 0) {
        process_pool[0].pid = 1;
        process_pool[0].activation = 0.6f;
        process_pool[0].state = 0;
        process_pool[0].parent = NULL;
        process_pool[0].next = NULL;
        process_count = 1;
    }
    return &process_pool[0];
}

/* ----- FS (triples) ----- */
typedef struct {
    char subj[32];
    char pred[32];
    char obj[64];
} triple_t;

static triple_t triple_pool[BTFOS_MAX_TRIPLES];
static unsigned int triple_count;
static unsigned int triple_alloc_next;

static triple_t *fs_alloc_triple(void) {
    if (triple_alloc_next >= BTFOS_MAX_TRIPLES)
        return NULL;
    return &triple_pool[triple_alloc_next++];
}

static void fs_insert_triple(triple_t *t) {
    (void)t;
    /* Already in pool; count used entries if needed */
    if (triple_count < BTFOS_MAX_TRIPLES)
        triple_count++;
}

void fs_init(void) {
    triple_count = 0;
    triple_alloc_next = 0;
    /* Root */
    triple_t *t = fs_alloc_triple();
    if (t) {
        int i;
        const char *s = "root", *p = "type", *o = "dir";
        for (i = 0; i < 31 && s[i]; i++)
            t->subj[i] = s[i];
        t->subj[i] = '\0';
        for (i = 0; i < 31 && p[i]; i++)
            t->pred[i] = p[i];
        t->pred[i] = '\0';
        for (i = 0; i < 63 && o[i]; i++)
            t->obj[i] = o[i];
        t->obj[i] = '\0';
        fs_insert_triple(t);
    }
}

/* Forward declare so fs_ingest_file can use it without implicit declaration */
const char *fs_lookup(const char *subj, const char *pred);

void fs_mkdir(const char *path) {
    triple_t *t = fs_alloc_triple();
    if (!t)
        return;
    int i;
    for (i = 0; i < 31 && path[i]; i++)
        t->subj[i] = path[i];
    t->subj[i] = '\0';
    {
        const char *p = "type";
        for (i = 0; i < 31 && p[i]; i++)
            t->pred[i] = p[i];
        t->pred[i] = '\0';
    }
    {
        const char *o = "dir";
        for (i = 0; i < 63 && o[i]; i++)
            t->obj[i] = o[i];
        t->obj[i] = '\0';
    }
    fs_insert_triple(t);
}

void fs_ingest_file(const char *path, const char *content) {
    triple_t *t1 = fs_alloc_triple();
    triple_t *t2 = fs_alloc_triple();
    if (!t1 || !t2)
        return;
    int i;
    for (i = 0; i < 31 && path[i]; i++)
        t1->subj[i] = path[i];
    t1->subj[i] = '\0';
    {
        const char *p = "type";
        for (i = 0; i < 31 && p[i]; i++)
            t1->pred[i] = p[i];
        t1->pred[i] = '\0';
    }
    {
        const char *ftype = "file";
        for (i = 0; i < 63 && ftype[i]; i++)
            t1->obj[i] = ftype[i];
        t1->obj[i] = '\0';
    }
    for (i = 0; i < 63 && path[i]; i++)
        t2->subj[i] = path[i];
    t2->subj[i] = '\0';
    {
        const char *pred_content = "content";
        for (i = 0; i < 31 && pred_content[i]; i++)
            t2->pred[i] = pred_content[i];
        t2->pred[i] = '\0';
    }
    for (i = 0; i < 63 && content && content[i]; i++)
        t2->obj[i] = content[i];
    t2->obj[i < 63 ? i : 63] = '\0';
    fs_insert_triple(t1);
    fs_insert_triple(t2);
}

const char *fs_lookup(const char *subj, const char *pred) {
    unsigned int k;
    for (k = 0; k < triple_count && k < BTFOS_MAX_TRIPLES; k++) {
        int i = 0;
        while (subj[i] && triple_pool[k].subj[i] && subj[i] == triple_pool[k].subj[i])
            i++;
        if (subj[i] != triple_pool[k].subj[i])
            continue;
        i = 0;
        while (pred[i] && triple_pool[k].pred[i] && pred[i] == triple_pool[k].pred[i])
            i++;
        if (pred[i] != triple_pool[k].pred[i])
            continue;
        return triple_pool[k].obj;
    }
    return NULL;
}

/* ----- Shell ----- */
void shell_ingest_command(const char *cmd) {
    triple_t *t = fs_alloc_triple();
    if (!t)
        return;
    int i;
    {
        const char *s = "shell";
        for (i = 0; i < 31 && s[i]; i++)
            t->subj[i] = s[i];
        t->subj[i] = '\0';
    }
    {
        const char *p = "exec";
        for (i = 0; i < 31 && p[i]; i++)
            t->pred[i] = p[i];
        t->pred[i] = '\0';
    }
    for (i = 0; i < 63 && cmd[i]; i++)
        t->obj[i] = cmd[i];
    t->obj[i < 63 ? i : 63] = '\0';
    fs_insert_triple(t);
}

static int shell_cmd_done;

void shell_init(void) {
    shell_cmd_done = 0;
}

void shell_run(void) {
    /* Single-shot: print prompt and ingest one "status" command */
    if (shell_cmd_done == 0) {
        shell_ingest_command("status");
        monitor_print("tick=0 procs=1\n");
        shell_cmd_done = 1;
    }
}

/* ----- Cognition loop (GOAT-TS) ----- */
void cognition_loop(int preset) {
    graph_t *g = graph_create();
    process_t *processes = process_list();

    for (process_t *p = processes; p; p = p->next) {
        graph_add_node(g, p->pid, p->activation, p->state);
        if (p->parent)
            graph_add_edge(g, p->pid, p->parent->pid, "child_of");
    }

    graph_spread_activation(g, NULL, 0.85f);
    graph_decay_states(g);

    if (preset >= BTFOS_BOOT_NORMAL)
        graph_apply_forces(g);

    if (preset == BTFOS_BOOT_FULL) {
        graph_curiosity(g);
        graph_reflection(g);
        graph_goal_generator(g);
        graph_sandbox_test(g);
    }

    for (process_t *p = processes; p; p = p->next) {
        p->activation = graph_get_activation(g, p->pid);
        p->state = graph_get_state(g, p->pid);
    }

    graph_destroy(g);
}

/* ----- Syscall stubs (unused params silenced) ----- */
void sys_exit(int code) {
    (void)code;
}

uint32_t sys_getpid(void) {
    return 0;
}

void sys_yield(void) {
}

int sys_write(int fd, const void *buf, size_t n) {
    (void)fd;
    (void)buf;
    (void)n;
    return 0;
}

int sys_read(int fd, void *buf, size_t n) {
    (void)fd;
    (void)buf;
    (void)n;
    return 0;
}

int sys_open(const char *path, int flags) {
    (void)path;
    (void)flags;
    return 0;
}

void sys_close(int fd) {
    (void)fd;
}

void sys_exec(const char *path) {
    (void)path;
}

/* ----- Kernel entry (called from boot.asm with magic, multiboot info) ----- */
void kernel_main(uint32_t magic, uint32_t mb_info) {
    (void)mb_info;
    serial_init();
    vga_clear();

    if (magic != 0x2BADB002) {
        monitor_print("Invalid multiboot magic\n");
        for (;;)
            __asm__ volatile ("hlt");
    }

    memory_init();
    process_init();
    fs_init();
    shell_init();

    int preset = BTFOS_BOOT_PRESET;
    cognition_loop(preset);

    monitor_print("BTFOS Ready\n> ");

    for (;;) {
        shell_run();
        cognition_loop(preset);
    }
}
