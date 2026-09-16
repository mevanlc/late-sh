# Instance defaults. Shared recipes live in common.mk.
# Compose settings live in .env.dev; application settings live in config.rs.
ENV_TEMPLATE ?= .env.dev
CHECK_INSTANCE ?= late-check
CHECK_PG_HOST_PORT ?= 55433

include common.mk
