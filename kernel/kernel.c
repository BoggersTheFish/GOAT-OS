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

/* ----- Timer interrupt: periodic cognition tick ----- */
static volatile uint32_t cognition_tick_pending;

/* Called from asm irq0_entry (IRQ0 = PIT) */
void timer_irq_handler(void) {
    cognition_tick_pending = 1;
    outb(0x20, 0x20);   /* PIC1 EOI */
}

/* IDT: 256 entries, 8 bytes each. Gate = offset_lo, selector, zero, type, offset_hi */
#define IDT_ENTRIES 256
typedef struct {
    uint16_t offset_lo;
    uint16_t selector;
    uint8_t  zero;
    uint8_t  type;
    uint16_t offset_hi;
} __attribute__((packed)) idt_entry_t;

static idt_entry_t idt[IDT_ENTRIES];
static struct {
    uint16_t limit;
    uint32_t base;
} __attribute__((packed)) idt_ptr;

extern void irq0_entry(void);

static void idt_set_gate(unsigned int i, uint32_t offset, uint16_t sel, uint8_t typ) {
    if (i >= IDT_ENTRIES) return;
    idt[i].offset_lo = (uint16_t)(offset & 0xFFFF);
    idt[i].offset_hi = (uint16_t)((offset >> 16) & 0xFFFF);
    idt[i].selector  = sel;
    idt[i].zero     = 0;
    idt[i].type     = typ;
}

static void idt_init(void) {
    uint32_t i;
    for (i = 0; i < IDT_ENTRIES; i++)
        idt_set_gate(i, 0, 0x08, 0);
    idt_set_gate(32, (uint32_t)irq0_entry, 0x08, 0x8E);  /* IRQ0: present, ring0, 32-bit interrupt gate */
    idt_ptr.limit = (uint16_t)(sizeof(idt) - 1);
    idt_ptr.base  = (uint32_t)&idt[0];
    __asm__ volatile ("lidt %0" : : "m"(idt_ptr));
}

/* PIC: remap to 0x20-0x2F, mask all but IRQ0 */
static void pic_init(void) {
    outb(0x20, 0x11);
    outb(0xA0, 0x11);
    outb(0x21, 0x20);
    outb(0xA1, 0x28);
    outb(0x21, 0x04);
    outb(0xA1, 0x02);
    outb(0x21, 0x01);
    outb(0xA1, 0x01);
    outb(0x21, 0xFE);   /* mask: allow IRQ0 only */
    outb(0xA1, 0xFF);
}

/* PIT channel 0: ~100 Hz (divisor 11932) for cognition tick */
#define PIT_DIVISOR 11932
static void pit_init(void) {
    outb(0x43, 0x36);
    outb(0x40, (uint8_t)(PIT_DIVISOR & 0xFF));
    outb(0x40, (uint8_t)((PIT_DIVISOR >> 8) & 0xFF));
}

/* ----- Graph (GOAT-TS): nodes = pid, activation, state, mass, position; edges = relations (from/to/weight). create/destroy/add_node/add_edge, spread_activation, decay_states, apply_forces, curiosity, reflection, goal_generator, sandbox_test, get_activation, get_state. ----- */
typedef struct graph_node_inner {
    uint32_t pid;
    float activation;
    int state;
    float mass;       /* for force sim / priority weighting */
    float pos_x;     /* 2D position for force-directed layout / affinity */
    float pos_y;
    struct graph_node_inner *next;
} graph_node_inner_t;

typedef struct graph_edge_inner {
    uint32_t from_pid;
    uint32_t to_pid;
    float weight;    /* relation strength (e.g. child_of -> 0.8) */
    struct graph_edge_inner *next;
} graph_edge_inner_t;

typedef struct graph_t {
    graph_node_inner_t *nodes;
    graph_node_inner_t *last;
    graph_edge_inner_t *edges;
    graph_edge_inner_t *edges_last;
} graph_t;

static graph_node_inner_t graph_node_pool[BTFOS_MAX_GRAPH_NODES];
static graph_edge_inner_t graph_edge_pool[BTFOS_MAX_EDGES];
static unsigned int graph_pool_idx;
static unsigned int graph_edge_idx;

/* Create fresh graph; resets node/edge pools for this cognition tick */
static graph_t *graph_create(void) {
    static graph_t g;
    g.nodes = NULL;
    g.last = NULL;
    g.edges = NULL;
    g.edges_last = NULL;
    graph_pool_idx = 0;
    graph_edge_idx = 0;
    return &g;
}

/* Free / tear-down (no-op with static pools; just reset is in create) */
static void graph_destroy(graph_t *g) {
    (void)g;
}

/* Add node: pid, activation, state, mass, position (for forces / layout) */
static void graph_add_node(graph_t *g, uint32_t pid, float act, int state, float mass, float pos_x, float pos_y) {
    if (graph_pool_idx >= BTFOS_MAX_GRAPH_NODES)
        return;
    graph_node_inner_t *n = &graph_node_pool[graph_pool_idx++];
    n->pid = pid;
    n->activation = act;
    n->state = state;
    n->mass = mass;
    n->pos_x = pos_x;
    n->pos_y = pos_y;
    n->next = NULL;
    if (g->last)
        g->last->next = n;
    else
        g->nodes = n;
    g->last = n;
}

/* Add edge: relation from -> to with strength (kind maps to weight; e.g. "child_of" -> 0.8) */
static void graph_add_edge(graph_t *g, uint32_t from, uint32_t to, const char *kind) {
    if (graph_edge_idx >= BTFOS_MAX_EDGES)
        return;
    float w = 0.5f;
    if (kind && kind[0] == 'c' && kind[1] == 'h') /* child_of */
        w = 0.8f;
    graph_edge_inner_t *e = &graph_edge_pool[graph_edge_idx++];
    e->from_pid = from;
    e->to_pid = to;
    e->weight = w;
    e->next = NULL;
    if (g->edges_last)
        g->edges_last->next = e;
    else
        g->edges = e;
    g->edges_last = e;
}

/* Find node by pid (used by spread/forces) */
static graph_node_inner_t *graph_find_node(graph_t *g, uint32_t pid) {
    graph_node_inner_t *n;
    for (n = g->nodes; n; n = n->next)
        if (n->pid == pid)
            return n;
    return NULL;
}

/* Spread activation along edges: high-activation nodes boost neighbors (scheduling priority) */
static void graph_spread_activation(graph_t *g, void *seeds, float decay) {
    (void)seeds;
    /* First pass: push activation along each edge (to += from * weight * factor) */
    graph_edge_inner_t *e;
    for (e = g->edges; e; e = e->next) {
        graph_node_inner_t *from_n = graph_find_node(g, e->from_pid);
        graph_node_inner_t *to_n = graph_find_node(g, e->to_pid);
        if (from_n && to_n && to_n->activation < 1.0f) {
            float add = from_n->activation * e->weight * 0.15f;
            if (to_n->activation + add > 1.0f)
                to_n->activation = 1.0f;
            else
                to_n->activation += add;
        }
    }
    /* Then decay all (so we don't blow up) */
    for (graph_node_inner_t *n = g->nodes; n; n = n->next) {
        if (n->activation > 0.02f)
            n->activation *= decay;
        if (n->activation > 1.0f)
            n->activation = 1.0f;
    }
}

/* Decay states: slowly reduce activation over time (forgetting) */
static void graph_decay_states(graph_t *g) {
    for (graph_node_inner_t *n = g->nodes; n; n = n->next) {
        if (n->activation > 0.05f)
            n->activation *= 0.97f;
    }
}

/* Apply forces: move position toward connected nodes (affinity / load-balance vibe) */
static void graph_apply_forces(graph_t *g) {
    graph_edge_inner_t *e;
    for (e = g->edges; e; e = e->next) {
        graph_node_inner_t *from_n = graph_find_node(g, e->from_pid);
        graph_node_inner_t *to_n = graph_find_node(g, e->to_pid);
        if (!from_n || !to_n)
            continue;
        /* Gentle pull: to moves a bit toward from (weighted by edge) */
        float dx = from_n->pos_x - to_n->pos_x;
        float dy = from_n->pos_y - to_n->pos_y;
        float step = 0.02f * e->weight;
        to_n->pos_x += dx * step;
        to_n->pos_y += dy * step;
    }
}

/* Curiosity: boost low-activation nodes (idle resource reuse) */
static void graph_curiosity(graph_t *g) {
    for (graph_node_inner_t *n = g->nodes; n; n = n->next) {
        if (n->activation < 0.3f)
            n->activation += 0.05f;
    }
}

/* Reflection: global tension proxy; if high, could log or nudge (bottleneck detection) */
static void graph_reflection(graph_t *g) {
    float sum = 0.0f;
    unsigned int cnt = 0;
    for (graph_node_inner_t *n = g->nodes; n; n = n->next) {
        sum += (1.0f - n->activation);
        cnt++;
    }
    (void)sum;
    (void)cnt;
    /* Tension = avg(1-act). High tension -> many low-act nodes. Stub: no log in kernel for now */
}

/* Goal generator: stub (could set goal state on high-priority nodes) */
static void graph_goal_generator(graph_t *g) {
    (void)g;
}

/* Sandbox test: stub (safe-to-apply change check) */
static void graph_sandbox_test(graph_t *g) {
    (void)g;
}

static float graph_get_activation(graph_t *g, uint32_t pid) {
    graph_node_inner_t *n = graph_find_node(g, pid);
    return n ? n->activation : 0.5f;
}

static int graph_get_state(graph_t *g, uint32_t pid) {
    graph_node_inner_t *n = graph_find_node(g, pid);
    return n ? n->state : 0;
}

/* ----- Process (stub list; mass/position for graph integration) ----- */
typedef struct process process_t;
struct process {
    uint32_t pid;
    float activation;
    int state;
    float mass;      /* for graph_add_node */
    float pos_x;
    float pos_y;
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

/* Return real process list: all entries in process_pool with pid != 0, linked by .next (ingest for graph) */
process_t *process_list(void) {
    if (process_count == 0) {
        process_pool[0].pid = 1;
        process_pool[0].activation = 0.6f;
        process_pool[0].state = 0;
        process_pool[0].mass = 1.0f;
        process_pool[0].pos_x = 0.0f;
        process_pool[0].pos_y = 0.0f;
        process_pool[0].parent = NULL;
        process_pool[0].next = NULL;
        process_count = 1;
    }
    /* Build linked list from pool (all active processes) */
    process_t *head = NULL;
    process_t *tail = NULL;
    unsigned int i;
    for (i = 0; i < BTFOS_MAX_PROCESSES; i++) {
        if (process_pool[i].pid == 0)
            continue;
        process_pool[i].next = NULL;
        if (tail)
            tail->next = &process_pool[i];
        else
            head = &process_pool[i];
        tail = &process_pool[i];
    }
    return head;
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

/* ----- Cognition loop (GOAT-TS): ingest processes -> graph, spread/decay/forces, then curiosity/reflection/goal/sandbox per preset, update processes ----- */
void cognition_loop(int preset) {
    graph_t *g = graph_create();
    process_t *processes = process_list();

    /* Ingest: add each process as node (pid, activation, state, mass, position), edges from parent */
    for (process_t *p = processes; p; p = p->next) {
        graph_add_node(g, p->pid, p->activation, p->state, p->mass, p->pos_x, p->pos_y);
        if (p->parent)
            graph_add_edge(g, p->pid, p->parent->pid, "child_of");
    }

    /* Always: spread activation (scheduling priority), then decay */
    graph_spread_activation(g, NULL, 0.85f);
    graph_decay_states(g);

    /* Forces if enabled (preset >= NORMAL): affinity / position updates */
    if (preset >= BTFOS_BOOT_NORMAL)
        graph_apply_forces(g);

    /* Full preset only: curiosity, reflection, goal generator, sandbox test */
    if (preset == BTFOS_BOOT_FULL) {
        graph_curiosity(g);
        graph_reflection(g);
        graph_goal_generator(g);
        graph_sandbox_test(g);
    }

    /* Write back: update process priorities (activation) and states from graph */
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

    /* Timer interrupt hook: IDT + PIC + PIT so cognition runs per tick */
    idt_init();
    pic_init();
    pit_init();
    cognition_tick_pending = 0;

    int preset = BTFOS_BOOT_PRESET;
    cognition_loop(preset);   /* one initial run before "Ready" */

    monitor_print("BTFOS Ready\n> ");

    __asm__ volatile ("sti");
    for (;;) {
        if (cognition_tick_pending) {
            cognition_tick_pending = 0;
            cognition_loop(preset);   /* run cognition per timer tick; updates process priorities/states from graph */
        }
        shell_run();
        __asm__ volatile ("hlt");
    }
}
