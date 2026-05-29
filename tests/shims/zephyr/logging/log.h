/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Host-side shim for <zephyr/logging/log.h>. history_lib.hpp pulls this
 * in but does not actually expand any LOG_* macros in its body. We
 * provide no-op stubs anyway so anything downstream that does call
 * LOG_INF / LOG_WRN / LOG_ERR / LOG_DBG silently compiles.
 */

#pragma once

#define LOG_MODULE_REGISTER(...)
#define LOG_MODULE_DECLARE(...)

#define LOG_DBG(...) ((void)0)
#define LOG_INF(...) ((void)0)
#define LOG_WRN(...) ((void)0)
#define LOG_ERR(...) ((void)0)
#define LOG_HEXDUMP_INF(...) ((void)0)
