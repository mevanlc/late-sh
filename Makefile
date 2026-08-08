# Instance defaults. Shared configuration and recipes live in common.mk.

INSTANCE ?= late

# Application listeners
LATE_SSH_PORT ?= 2222
LATE_API_PORT ?= 4001
LATE_WEB_PORT ?= 3000
LATE_IRC_PORT ?= 6667
LATE_IRC_TLS_HOST_PORT ?= 6697

# Supporting services
LATE_PG_HOST_PORT ?= 5433
LATE_ICECAST_HOST_PORT ?= 8000
LATE_LIVEKIT_HOST_PORT ?= 7880
LATE_LIVEKIT_RTC_TCP_PORT ?= 7881
LATE_LIVEKIT_RTC_UDP_PORT ?= 7882

# Check database
CHECK_INSTANCE ?= late-check
CHECK_PG_HOST_PORT ?= 55433

include common.mk
