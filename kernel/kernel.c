/*
 * BTFOS Kernel - Graph-Driven Cognition OS
 * Copyright (c) Ben Michalek (BoggersTheFish). MIT License.
 * Single-file kernel: compiles with gcc -m32 -I include -ffreestanding -nostdlib.
 * Boot entry: kernel_main(uint32_t magic, uint32_t mb_info) from boot.asm.
 */

#include <stdint.h>
#include <stddef.h>
#include "btfos_config.h"

/* ==== Tiny utils (no libc) ==== */
static void mem_zero(void *p, size_t n) {
    uint8_t *b = (uint8_t *)p;
    for (size_t i = 0; i < n; i++)
        b[i] = 0;
}

static void u32_to_dec(uint32_t v, char out[12]) {
    char tmp[12];
    unsigned int i = 0;
    if (v == 0) {
        out[0] = '0';
        out[1] = '\0';
        return;
    }
    while (v && i < sizeof(tmp)) {
        tmp[i++] = (char)('0' + (v % 10));
        v /= 10;
    }
    unsigned int o = 0;
    while (i > 0)
        out[o++] = tmp[--i];
    out[o] = '\0';
}

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

/* ----- Timer interrupt (IRQ0, IDT 32): PIT calls cognition_loop every ~10 ms (100 Hz) ----- */
static volatile uint32_t ticks;
/* Preset for cognition_loop; set once in kernel_main so timer handler can call cognition_loop. */
static int s_cognition_preset;

/* Called from asm irq0_entry (IRQ0 = PIT). Cognition runs from timer only, not main loop. */
void timer_irq_handler(void) {
    ticks++;
    cognition_loop(s_cognition_preset);
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

/* PIT channel 0 (ports 0x40/0x43): 100 Hz so cognition_loop runs every ~10 ms (timer tick).
   Divisor = 1193182/100 = 11932. */
#define PIT_DIVISOR 11932
static void pit_init(void) {
    outb(0x43, 0x36);   /* channel 0, mode 3, binary */
    outb(0x40, (uint8_t)(PIT_DIVISOR & 0xFF));
    outb(0x40, (uint8_t)((PIT_DIVISOR >> 8) & 0xFF));
}

/* ----- Keyboard (very tiny, polling scancodes) ----- */
static const char scancode_to_ascii[128] = {
    0,  27, '1','2','3','4','5','6','7','8','9','0','-','=', '\b',
    '\t','q','w','e','r','t','y','u','i','o','p','[',']','\n', 0,
    'a','s','d','f','g','h','j','k','l',';','\'','`', 0,'\\',
    'z','x','c','v','b','n','m',',','.','/', 0, '*', 0, ' ',
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
};

static int kbd_has_data(void) {
    return (inb(0x64) & 1) != 0;
}

static char kbd_read_char_blocking(void) {
    for (;;) {
        if (!kbd_has_data()) {
            __asm__ volatile ("hlt");
            continue;
        }
        uint8_t sc = inb(0x60);
        if (sc & 0x80) /* key up */
            continue;
        if (sc < 128) {
            char c = scancode_to_ascii[sc];
            if (c)
                return c;
        }
    }
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
typedef enum { PROC_READY = 0, PROC_RUN = 1, PROC_ZOMBIE = 2 } proc_state_t;
struct process {
    uint32_t pid;
    float activation;
    int state;        /* proc_state_t */
    float mass;      /* for graph_add_node */
    float pos_x;
    float pos_y;
    char name[16];
    process_t *parent;
    process_t *next;
};

static process_t process_pool[BTFOS_MAX_PROCESSES];
static unsigned int process_count;
static uint32_t next_pid = 1;
static uint32_t current_pid = 0;

void process_init(void) {
    process_count = 0;
    next_pid = 1;
    current_pid = 0;
    for (unsigned int i = 0; i < BTFOS_MAX_PROCESSES; i++)
        process_pool[i].pid = 0;
}

static process_t *process_get(uint32_t pid) {
    for (unsigned int i = 0; i < BTFOS_MAX_PROCESSES; i++)
        if (process_pool[i].pid == pid)
            return &process_pool[i];
    return NULL;
}

static process_t *process_spawn(const char *name, process_t *parent) {
    for (unsigned int i = 0; i < BTFOS_MAX_PROCESSES; i++) {
        if (process_pool[i].pid != 0)
            continue;
        process_pool[i].pid = next_pid++;
        process_pool[i].activation = 0.6f;
        process_pool[i].state = PROC_READY;
        process_pool[i].mass = 1.0f;
        process_pool[i].pos_x = (float)(process_pool[i].pid % 7);
        process_pool[i].pos_y = (float)((process_pool[i].pid / 7) % 7);
        mem_zero(process_pool[i].name, sizeof(process_pool[i].name));
        if (name) {
            for (unsigned int k = 0; k < sizeof(process_pool[i].name) - 1 && name[k]; k++)
                process_pool[i].name[k] = name[k];
        }
        process_pool[i].parent = parent;
        process_pool[i].next = NULL;
        process_count++;
        return &process_pool[i];
    }
    return NULL;
}

static void process_kill(uint32_t pid) {
    for (unsigned int i = 0; i < BTFOS_MAX_PROCESSES; i++) {
        if (process_pool[i].pid == pid) {
            process_pool[i].pid = 0;
            process_pool[i].state = PROC_ZOMBIE;
            process_pool[i].activation = 0.0f;
            process_count = (process_count > 0) ? (process_count - 1) : 0;
            return;
        }
    }
}

/* Return real process list: all entries in process_pool with pid != 0, linked by .next (ingest for graph) */
process_t *process_list(void) {
    if (process_count == 0) {
        (void)process_spawn("init", NULL);
        current_pid = 1;
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

static void fs_write_cstr(char *dst, size_t cap, const char *src) {
    size_t i = 0;
    if (!dst || cap == 0) return;
    while (i + 1 < cap && src && src[i]) {
        dst[i] = src[i];
        i++;
    }
    dst[i] = '\0';
}

/* Ingest syscall as triple: subj = process name or pid, pred = "syscall", obj = syscall name + args. */
static void fs_ingest_syscall_triple(const char *subj, const char *obj_str) {
    triple_t *t = fs_alloc_triple();
    if (!t)
        return;
    fs_write_cstr(t->subj, sizeof(t->subj), subj ? subj : "0");
    fs_write_cstr(t->pred, sizeof(t->pred), "syscall");
    fs_write_cstr(t->obj, sizeof(t->obj), obj_str ? obj_str : "");
    fs_insert_triple(t);
}

/* Legacy: ingest by pid and name+detail (builds obj = name + " " + detail). */
static void fs_ingest_syscall(uint32_t pid, const char *name, const char *detail) {
    char pidbuf[12];
    u32_to_dec(pid, pidbuf);
    process_t *p = process_get(pid);
    const char *subj = (p && p->name[0]) ? p->name : pidbuf;
    char obj[64];
    size_t o = 0;
    if (name)
        for (; o + 1 < sizeof(obj) && name[o]; o++) obj[o] = name[o];
    if (detail && detail[0] && o < sizeof(obj) - 2) {
        obj[o++] = ' ';
        for (size_t i = 0; o + 1 < sizeof(obj) && detail[i]; i++, o++) obj[o] = detail[i];
    }
    obj[o] = '\0';
    fs_ingest_syscall_triple(subj, obj);
}

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

/* ----- Syscall prototypes (used by shell) ----- */
int sys_write(int fd, const void *buf, size_t n);
int sys_read(int fd, void *buf, size_t n);
void sys_exit(int code);
int sys_open(const char *path, int flags);
int sys_close(int fd);
void sys_exec(const char *path);
uint32_t sys_getpid(void);
void sys_yield(void);

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

static char shell_line[80];

void shell_init(void) {
    /* nothing yet */
}

static void shell_prompt(void) { monitor_print("> "); }

static int shell_readline(char *buf, unsigned int cap) {
    if (!buf || cap < 2) return 0;
    unsigned int n = 0;
    for (;;) {
        char c = kbd_read_char_blocking();
        if (c == '\r') c = '\n';
        if (c == '\n') {
            monitor_print("\n");
            buf[n] = '\0';
            return (int)n;
        }
        if (c == '\b') {
            if (n > 0) {
                n--;
                monitor_print("\b \b");
            }
            continue;
        }
        if (n + 1 < cap) {
            buf[n++] = c;
            char out[2] = { c, 0 };
            monitor_print(out);
        }
    }
}

void shell_run(void) {
    shell_prompt();
    int n = shell_readline(shell_line, (unsigned int)sizeof(shell_line));
    if (n <= 0)
        return;

    shell_ingest_command(shell_line);

    /* Built-ins: help | status | ps | run <task> | exit */
    if (shell_line[0] == 'h') {
        monitor_print("help: status | ps | run <task> | echo <text> | read | exit\n");
        return;
    }
    if (shell_line[0] == 's') {
        monitor_print("ticks=");
        char tb[12];
        u32_to_dec(ticks, tb);
        monitor_print(tb);
        monitor_print(" procs=");
        char pb[12];
        u32_to_dec(process_count, pb);
        monitor_print(pb);
        monitor_print("\n");
        return;
    }
    if (shell_line[0] == 'p' && shell_line[1] == 's') {
        process_t *p = process_list();
        while (p) {
            monitor_print("pid=");
            char b[12];
            u32_to_dec(p->pid, b);
            monitor_print(b);
            monitor_print(" act=");
            monitor_print(p->activation > 0.5f ? "hi" : "lo");
            monitor_print(" name=");
            monitor_print(p->name);
            monitor_print("\n");
            p = p->next;
        }
        return;
    }
    if (shell_line[0] == 'r' && shell_line[1] == 'u' && shell_line[2] == 'n' && shell_line[3] == ' ') {
        /* Accept both: \"run <task>\" and \"run exec <task>\" */
        if (shell_line[4] == 'e' && shell_line[5] == 'x' && shell_line[6] == 'e' && shell_line[7] == 'c' && shell_line[8] == ' ') {
            sys_exec(&shell_line[9]);
            return;
        }
        sys_exec(&shell_line[4]);
        return;
    }
    if (shell_line[0] == 'e' && shell_line[1] == 'x' && shell_line[2] == 'i' && shell_line[3] == 't') {
        monitor_print("bye.\n");
        for (;;)
            __asm__ volatile ("hlt");
    }
    /* echo <text>: write to stdout via sys_write(1, ...). */
    if (shell_line[0] == 'e' && shell_line[1] == 'c' && shell_line[2] == 'h' && shell_line[3] == 'o' && shell_line[4] == ' ') {
        static char echo_buf[80];
        size_t i = 0;
        size_t j = 5;
        while (shell_line[j] && i < sizeof(echo_buf) - 2)
            echo_buf[i++] = shell_line[j++];
        echo_buf[i++] = '\n';
        echo_buf[i] = '\0';
        sys_write(1, echo_buf, i);
        return;
    }
    /* read: read up to 10 bytes from stdin via sys_read(0, ...), then print result. */
    if (shell_line[0] == 'r' && shell_line[1] == 'e' && shell_line[2] == 'a' && shell_line[3] == 'd' && shell_line[4] == '\0') {
        static char read_buf[11];
        int nr = sys_read(0, read_buf, 10);
        if (nr > 0) {
            for (int i = 0; i < nr; i++) {
                char out[2] = { read_buf[i], 0 };
                monitor_print(out);
            }
        }
        return;
    }
    monitor_print("unknown. try: help\n");
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

    /* Scheduling decision: choose highest-activation runnable process */
    float best = -1.0f;
    uint32_t best_pid = current_pid;
    for (process_t *p = processes; p; p = p->next) {
        if (p->state == PROC_ZOMBIE)
            continue;
        if (p->activation > best) {
            best = p->activation;
            best_pid = p->pid;
        }
    }
    current_pid = best_pid;
    process_t *curp = process_get(current_pid);
    if (curp)
        curp->state = PROC_RUN;

    graph_destroy(g);
}

/* ----- Real syscalls: minimal implementations, each ingests triple (subj=pid/name, pred=syscall, obj=name+args) ----- */

/* Serial transmit: wait for THRE (LSR bit 5 at 0x3FD) then send one byte to 0x3F8. */
static void serial_putc_blocking(char c) {
    while ((inb(SERIAL_PORT + 5) & 0x20) == 0)
        ;
    outb(SERIAL_PORT, c);
}

/* sys_write: if fd==1, write buf to serial 0x3F8 (loop until each byte transmitted). Else return -1. */
int sys_write(int fd, const void *buf, size_t n) {
    if (fd != 1 || !buf) {
        char obj[32];
        obj[0] = 'w'; obj[1] = 'r'; obj[2] = 'i'; obj[3] = 't'; obj[4] = 'e';
        obj[5] = ' '; obj[6] = '0' + (fd % 10); obj[7] = ' '; obj[8] = '0'; obj[9] = '\0';
        process_t *p = process_get(current_pid);
        char pidbuf[12];
        u32_to_dec(current_pid, pidbuf);
        fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
        return fd == 1 ? 0 : -1;
    }
    const char *c = (const char *)buf;
    for (size_t i = 0; i < n; i++)
        serial_putc_blocking(c[i]);
    char obj[32];
    size_t o = 0;
    obj[o++] = 'w'; obj[o++] = 'r'; obj[o++] = 'i'; obj[o++] = 't'; obj[o++] = 'e';
    obj[o++] = ' '; obj[o++] = '1'; obj[o++] = ' ';
    char nb[12];
    u32_to_dec((uint32_t)n, nb);
    for (size_t i = 0; nb[i] && o < sizeof(obj) - 1; i++) obj[o++] = nb[i];
    obj[o] = '\0';
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
    process_t *pp = process_get(current_pid);
    if (pp && pp->activation < 1.0f)
        pp->activation += 0.02f;
    return (int)n;
}

/* sys_read: if fd==0, read from keyboard port 0x60 (poll 0x64 until byte available), fill buf. */
int sys_read(int fd, void *buf, size_t n) {
    if (fd != 0 || !buf || n == 0) {
        process_t *p = process_get(current_pid);
        char pidbuf[12];
        u32_to_dec(current_pid, pidbuf);
        fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, "read 0 0");
        return -1;
    }
    char *c = (char *)buf;
    size_t i = 0;
    while (i < n) {
        char ch = kbd_read_char_blocking();
        c[i++] = ch;
        if (ch == '\n')
            break;
    }
    char obj[24];
    obj[0] = 'r'; obj[1] = 'e'; obj[2] = 'a'; obj[3] = 'd'; obj[4] = ' ';
    obj[5] = '0'; obj[6] = ' ';
    char nb[12];
    u32_to_dec((uint32_t)i, nb);
    size_t o = 7;
    for (size_t k = 0; nb[k] && o < sizeof(obj) - 1; k++) obj[o++] = nb[k];
    obj[o] = '\0';
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
    if (p && p->activation < 1.0f)
        p->activation += 0.01f;
    return (int)i;
}

/* sys_exit: kill current process (remove from list, free slot). Ingest "exit <code>". */
void sys_exit(int code) {
    char obj[16];
    obj[0] = 'e'; obj[1] = 'x'; obj[2] = 'i'; obj[3] = 't'; obj[4] = ' ';
    char cb[12];
    u32_to_dec((uint32_t)(code & 0xFF), cb);
    size_t o = 5;
    for (size_t i = 0; cb[i] && o < sizeof(obj) - 1; i++) obj[o++] = cb[i];
    obj[o] = '\0';
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
    uint32_t pid = current_pid;
    process_kill(pid);
    if (pid == 1) {
        (void)process_spawn("init", NULL);
        current_pid = 1;
    } else {
        current_pid = 0;
        process_t *list = process_list();
        if (list)
            current_pid = list->pid;
    }
}

uint32_t sys_getpid(void) {
    return current_pid;
}

void sys_yield(void) {
    (void)0;
}

/* sys_open: stub FS — return dummy fd 1 if path exists in triple store, else -1. */
int sys_open(const char *path, int flags) {
    (void)flags;
    const char *t = fs_lookup(path ? path : "", "type");
    char obj[48];
    obj[0] = 'o'; obj[1] = 'p'; obj[2] = 'e'; obj[3] = 'n'; obj[4] = ' ';
    size_t o = 5;
    if (path)
        for (size_t i = 0; path[i] && o < sizeof(obj) - 1; i++) obj[o++] = path[i];
    obj[o] = '\0';
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
    return t ? 1 : -1;
}

/* sys_close: stub — return 0. */
int sys_close(int fd) {
    char obj[16];
    obj[0] = 'c'; obj[1] = 'l'; obj[2] = 'o'; obj[3] = 's'; obj[4] = 'e';
    obj[5] = ' '; obj[6] = '0' + (fd % 10); obj[7] = '\0';
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, obj);
    return 0;
}

/* sys_exec: stub — print "exec stub" to serial only and return. */
void sys_exec(const char *path) {
    (void)path;
    const char *msg = "exec stub\n";
    for (size_t i = 0; msg[i]; i++)
        serial_putc_blocking(msg[i]);
    process_t *p = process_get(current_pid);
    char pidbuf[12];
    u32_to_dec(current_pid, pidbuf);
    fs_ingest_syscall_triple((p && p->name[0]) ? p->name : pidbuf, "exec stub");
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
    ticks = 0;

    /* Cognition runs from PIT timer only (timer_irq_handler), not from main loop. */
    s_cognition_preset = BTFOS_BOOT_PRESET;
    cognition_loop(s_cognition_preset);   /* one initial run before "Ready" */

    monitor_print("BTFOS Ready\n> ");

    __asm__ volatile ("sti");
    for (;;) {
        shell_run();
        __asm__ volatile ("hlt");
    }
}
