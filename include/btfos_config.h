/* BTFOS config: presets and feature flags. MIT License. */
#ifndef BTFOS_CONFIG_H
#define BTFOS_CONFIG_H

/* Presets: BTFOS_BOOT (lightweight), BTFOS_NORMAL, BTFOS_FULL */
#define BTFOS_PRESET BTFOS_NORMAL

#if BTFOS_PRESET == BTFOS_BOOT
#define BTFOS_TICKS_PER_LOOP     1
#define BTFOS_ENABLE_FORCES      0
#define BTFOS_ENABLE_REFLECTION  0
#define BTFOS_ENABLE_CURIOSITY  0
#define BTFOS_ENABLE_GOALS      0
#define BTFOS_SHARD_COUNT       1
#elif BTFOS_PRESET == BTFOS_NORMAL
#define BTFOS_TICKS_PER_LOOP     10
#define BTFOS_ENABLE_FORCES      1
#define BTFOS_ENABLE_REFLECTION  1
#define BTFOS_ENABLE_CURIOSITY  1
#define BTFOS_ENABLE_GOALS      1
#define BTFOS_SHARD_COUNT       2
#else /* BTFOS_FULL */
#define BTFOS_TICKS_PER_LOOP     100
#define BTFOS_ENABLE_FORCES      1
#define BTFOS_ENABLE_REFLECTION  1
#define BTFOS_ENABLE_CURIOSITY  1
#define BTFOS_ENABLE_GOALS      1
#define BTFOS_SHARD_COUNT       4
#endif

#define BTFOS_MAX_PROCESSES     32
#define BTFOS_MAX_PAGES         256
#define BTFOS_MAX_GRAPH_NODES   512
#define BTFOS_MAX_EDGES         1024
#define BTFOS_MAX_TRIPLES       256
#define BTFOS_MAX_SYSCALL_LOG   64
#define BTFOS_SHELL_BUF         128
#define BTFOS_PLUGIN_DIR        "/plugins"
#define BTFOS_KERNEL_SOURCE_TAG "btfos_kernel_v1"

#endif
