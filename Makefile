## BBLBB 根 Makefile
##
## 统一入口：make <target>
## 所有子命令失败时立即终止（MAKEFLAGS += --no-print-directory）
## 用途参见 `make help`

SHELL := /bin/bash
.DEFAULT_GOAL := help
MAKEFLAGS += --no-print-directory

PROJECT_ROOT := $(CURDIR)
BACKEND_DIR := $(CURDIR)/backend
FRONTEND_DIR := $(CURDIR)/frontend
PROTOTYPE_DIR := $(CURDIR)/prototype
OPENAPI_FILE := $(CURDIR)/openapi/openapi.yaml
MIGRATIONS_DIR := $(CURDIR)/migrations

# 颜色（通过 printf 输出，兼容 macOS/Linux）
BOLD := \033[1m
GREEN := \033[32m
YELLOW := \033[33m
RED := \033[31m
RESET := \033[0m

##@ 帮助
help: ## 显示此帮助信息
	@printf "\n$(BOLD)BBLBB 根命令$(RESET)\n"
	@printf "$(BOLD)用法:$(RESET) make $(YELLOW)<target>$(RESET)\n"
	@printf "$(BOLD)目标:$(RESET)\n"
	@grep -E '^##@ |^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*## "} \
			/^##@ / {sub(/^##@ /, ""); printf "\n$(BOLD)%s$(RESET)\n", $$0; next} \
			{printf "  $(GREEN)%-20s$(RESET) %s\n", $$1, $$2}'
	@printf "$(BOLD)示例:$(RESET)\n"
	@printf "  make check          # 运行全部检查\n"
	@printf "  make test           # 运行全部测试\n"
	@printf "  make build          # 构建后端和前端\n"
	@printf "  make migrate        # 应用 SQLite 迁移\n"
	@printf "  make dev            # 启动前端开发服务器\n"
	@printf "  make clean          # 清理构建产物\n"
	@printf "  make install        # 安装前端/原型依赖\n"
	@printf "\n"

##@ 开发
dev: ## 启动前端开发服务器（后端需单独运行）
	@printf "$(YELLOW)>>> 启动前端开发服务器...$(RESET)\n"
	@cd $(FRONTEND_DIR) && npm run dev

dev-backend: ## 启动后端开发服务器
	@printf "$(YELLOW)>>> 启动后端开发服务器...$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo run

##@ 检查
check: check-backend check-migrations check-frontend check-prototype check-openapi check-contract check-roadmap check-docs check-secrets ## 运行全部检查

check-backend: ## 后端 fmt + clippy + 编译检查 + 领域层依赖边界 + 事务 IO 边界
	@printf "$(GREEN)>>> [check-backend] Rust fmt + clippy + check$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo fmt --all -- --check
	@$(MAKE) check-domain
	@$(MAKE) check-tx-io
	@cd $(BACKEND_DIR) && cargo clippy --workspace --all-targets --all-features -- -D warnings
	@cd $(BACKEND_DIR) && cargo check --all-features

check-domain: ## 领域层依赖边界扫描（禁止 axum/sqlx/SMTP/S3/环境变量）
	@printf "$(GREEN)>>> [check-domain] 领域层依赖边界$(RESET)\n"
	@if grep -rnE "use (axum|sqlx)|::(axum|sqlx)|std::env" $(BACKEND_DIR)/src/domain/; then \
		echo "$(RED)错误：backend/src/domain/ 不得依赖 axum、sqlx 或环境变量$(RESET)"; exit 1; \
	else \
		echo "领域层依赖边界 OK（无 axum/sqlx/环境变量）"; \
	fi

check-tx-io: ## 写事务 IO 边界扫描（禁止事务内 SMTP/S3/AI/视频/图片处理，M01-JOBS-07）
	@printf "$(GREEN)>>> [check-tx-io] 写事务 IO 边界$(RESET)\n"
	@ruby scripts/check-tx-io.rb

check-migrations: ## 三数据库迁移结构等价断言（M01-DB-09）
	@printf "$(GREEN)>>> [check-migrations] 迁移结构等价断言$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo test --test migration_equivalence --quiet 2>&1 | tail -n 8

check-frontend: ## 前端 Svelte check + TypeScript 类型检查
	@printf "$(GREEN)>>> [check-frontend] SvelteKit check$(RESET)\n"
	@cd $(FRONTEND_DIR) && npm ci --silent
	@cd $(FRONTEND_DIR) && npm run check

check-prototype: ## 原型 render + interaction 检查
	@printf "$(GREEN)>>> [check-prototype] 原型渲染 + 交互检查$(RESET)\n"
	@cd $(PROTOTYPE_DIR) && npm ci --silent
	@cd $(PROTOTYPE_DIR) && npm run check:all

check-openapi: ## OpenAPI YAML 解析 + operationId 唯一性检查
	@printf "$(GREEN)>>> [check-openapi] OpenAPI 契约校验$(RESET)\n"
	@ruby -e '\
		require "yaml"; \
		doc = YAML.safe_load(File.read("$(OPENAPI_FILE)"), aliases: true); \
		abort "OpenAPI must be a mapping" unless doc.is_a?(Hash); \
		abort "Missing openapi version" unless doc.fetch("openapi","").start_with?("3."); \
		abort "Missing paths" unless doc["paths"].is_a?(Hash); \
		ops = []; \
		doc["paths"].each { |path, methods| \
			methods.each { |method, spec| \
				next unless %w[get post put patch delete head options].include?(method); \
				op_id = spec["operationId"]; \
				abort "Missing operationId at #{method.upcase} #{path}" unless op_id; \
				ops << op_id; \
			} \
		}; \
		dups = ops.group_by { |x| x }.select { |_, v| v.size > 1 }; \
		abort "Duplicate operationIds: #{dups.keys.join(", ")}" unless dups.empty?; \
		puts "OpenAPI: #{ops.size} operations, all unique"; \
	'
	@if [ -f "$(PROJECT_ROOT)/scripts/sync-operation-coverage.rb" ]; then \
		ruby $(PROJECT_ROOT)/scripts/sync-operation-coverage.rb --check; \
	fi

check-contract: ## 契约治理脚本（错误码/写契约/路由覆盖/权限矩阵/状态枚举/TS 类型）
	@printf "$(GREEN)>>> [check-contract] 契约治理脚本$(RESET)\n"
	@ruby $(PROJECT_ROOT)/scripts/check-error-codes.rb
	@ruby $(PROJECT_ROOT)/scripts/check-write-contract.rb
	@ruby $(PROJECT_ROOT)/scripts/check-route-coverage.rb
	@ruby $(PROJECT_ROOT)/scripts/check-permission-matrix.rb
	@ruby $(PROJECT_ROOT)/scripts/check-state-enums.rb
	@ruby $(PROJECT_ROOT)/scripts/generate-ts-types.rb --check

check-docs: ## Markdown 链接检查
	@printf "$(GREEN)>>> [check-docs] Markdown 链接检查$(RESET)\n"
	@if command -v lychee >/dev/null 2>&1; then \
		lychee --offline --no-progress './README.md' './docs/**/*.md' './dev/**/*.md'; \
	else \
		printf "$(YELLOW)    lychee 未安装，跳过链接检查$(RESET)\n"; \
	fi

check-secrets: ## Secret 扫描（检查是否有泄露的密钥/Token）
	@printf "$(GREEN)>>> [check-secrets] Secret 扫描$(RESET)\n"
	@! grep -rn --include='*.rs' --include='*.ts' --include='*.js' --include='*.json' --include='*.yaml' --include='*.yml' \
		-E '(AKIA[0-9A-Z]{16}|sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|-----BEGIN (RSA |EC )?PRIVATE KEY-----)' \
		$(BACKEND_DIR)/src $(FRONTEND_DIR)/src $(PROJECT_ROOT)/openapi 2>/dev/null \
		|| { printf "$(RED)ERROR: 检测到疑似 Secret！$(RESET)\n"; exit 1; }
	@printf "    未检测到已知 Secret 模式\n"

check-roadmap: ## 路线图校验
	@printf "$(GREEN)>>> [check-roadmap] 路线图一致性校验$(RESET)\n"
	@if [ -f "$(PROJECT_ROOT)/scripts/check-roadmap.rb" ]; then \
		ruby $(PROJECT_ROOT)/scripts/check-roadmap.rb; \
	else \
		printf "$(YELLOW)    check-roadmap.rb 不存在$(RESET)\n"; \
	fi

##@ 测试
test: test-backend test-frontend test-prototype ## 运行全部测试

test-backend: ## 后端测试
	@printf "$(GREEN)>>> [test-backend] cargo test --all-features$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo test --all-features

test-frontend: ## 前端测试
	@printf "$(GREEN)>>> [test-frontend] 前端单测$(RESET)\n"
	@cd $(FRONTEND_DIR) && npm test --if-present

test-prototype: ## 原型检查
	@printf "$(GREEN)>>> [test-prototype] 原型检查$(RESET)\n"
	@cd $(PROTOTYPE_DIR) && npm run check:all

##@ 构建
build: build-backend build-frontend ## 构建后端和前端

build-backend: ## 后端 release 构建
	@printf "$(GREEN)>>> [build-backend] cargo build --release$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo build --release

build-frontend: ## 前端构建
	@printf "$(GREEN)>>> [build-frontend] SvelteKit adapter-node build$(RESET)\n"
	@cd $(FRONTEND_DIR) && npm ci --silent
	@cd $(FRONTEND_DIR) && npm run build

##@ 数据库迁移
migrate: migrate-sqlite ## 应用迁移（默认 SQLite）

migrate-sqlite: ## 应用 SQLite 迁移到空库
	@printf "$(GREEN)>>> [migrate-sqlite] 应用 SQLite 迁移$(RESET)\n"
	@DB=$${BBLBB_DB:-/tmp/bblbb.sqlite}; \
		rm -f $$DB; \
		for f in $(MIGRATIONS_DIR)/sqlite/*.sql; do \
			echo "  applying: $$f"; \
			sqlite3 -bail $$DB < $$f || { printf "$(RED)FAILED: $$f$(RESET)\n"; exit 1; }; \
		done; \
		echo "  foreign key check:"; \
		sqlite3 $$DB 'PRAGMA foreign_key_check;'; \
		printf "$(GREEN)  SQLite 迁移完成: $$DB$(RESET)\n"

migrate-check-sqlite: ## 检查 SQLite 迁移（不应用）
	@printf "$(GREEN)>>> [migrate-check-sqlite] 检查迁移$(RESET)\n"
	@DB=$${BBLBB_DB:-/tmp/bblbb-check.sqlite}; \
		rm -f $$DB; \
		for f in $(MIGRATIONS_DIR)/sqlite/*.sql; do \
			sqlite3 -bail $$DB < $$f || { printf "$(RED)FAILED: $$f$(RESET)\n"; exit 1; }; \
		done; \
		test -z "$$(sqlite3 $$DB 'PRAGMA foreign_key_check;')" && echo "  OK" || { printf "$(RED)FK check failed$(RESET)\n"; exit 1; }; \
		rm -f $$DB

##@ 清理
clean: ## 清理构建产物
	@printf "$(YELLOW)>>> 清理构建产物...$(RESET)\n"
	@cd $(BACKEND_DIR) && cargo clean 2>/dev/null || true
	@rm -rf $(FRONTEND_DIR)/build $(FRONTEND_DIR)/.svelte-kit 2>/dev/null || true
	@printf "$(GREEN)完成$(RESET)\n"

##@ 安装
install: ## 安装依赖
	@printf "$(GREEN)>>> 安装前端依赖...$(RESET)\n"
	@cd $(FRONTEND_DIR) && npm ci
	@cd $(PROTOTYPE_DIR) && npm ci
	@printf "$(GREEN)>>> Rust 依赖将由 cargo 自动拉取$(RESET)\n"
