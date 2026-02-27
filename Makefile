.PHONY: dev build run kill clean install logs help release

# Couleurs
GREEN := \033[0;32m
YELLOW := \033[0;33m
NC := \033[0m

help: ## Affiche l'aide
	@echo "$(GREEN)Dictea$(NC) - Commandes disponibles:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-12s$(NC) %s\n", $$1, $$2}'
	@echo ""

dev: ## Lance en mode développement (hot reload)
	pnpm tauri dev

build: ## Build l'application (release)
	pnpm tauri build

run: build-frontend ## Lance le binaire debug (avec frontend buildé)
	./src-tauri/target/debug/dictea

build-frontend: ## Build le frontend
	pnpm build

run-release: ## Lance le binaire release
	./src-tauri/target/release/dictea

kill: ## Tue tous les processus Dictea
	@pkill -f "dictea" 2>/dev/null || echo "Aucun processus Dictea trouvé"

clean: ## Nettoie les builds
	cd src-tauri && cargo clean
	rm -rf dist

install: ## Installe les dépendances
	pnpm install
	cd src-tauri && cargo fetch

logs: ## Affiche les logs en temps réel
	@echo "Logs Dictea (Ctrl+C pour quitter):"
	@tail -f /tmp/dictea.log 2>/dev/null || echo "Pas de fichier de log"

build-rust: ## Build uniquement le backend Rust
	cd src-tauri && cargo build

build-rust-release: ## Build le backend Rust en release
	cd src-tauri && cargo build --release

open-settings: ## Ouvre les paramètres Accessibilité macOS
	open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"

release: ## Crée un tag et déclenche la release CI/CD (usage: make release VERSION=0.2.0)
	@if [ -z "$(VERSION)" ]; then echo "$(YELLOW)Usage: make release VERSION=x.y.z$(NC)"; exit 1; fi
	@echo "$(GREEN)Release v$(VERSION)$(NC)"
	@# Bump version dans Cargo.toml (line 3: version = "x.y.z")
	@sed -i '' '3s/version = "[^"]*"/version = "$(VERSION)"/' src-tauri/Cargo.toml
	@# Bump version dans tauri.conf.json (line 4: "version": "x.y.z")
	@sed -i '' '4s/"version": "[^"]*"/"version": "$(VERSION)"/' src-tauri/tauri.conf.json
	@# Bump version dans package.json (line 3: "version": "x.y.z")
	@sed -i '' '3s/"version": "[^"]*"/"version": "$(VERSION)"/' package.json
	git add src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json
	git diff --cached --quiet || git commit -m "release: v$(VERSION)"
	@# Delete tag locally and remotely if it already exists, then recreate
	-@git tag -d "v$(VERSION)" 2>/dev/null
	-@git push origin :refs/tags/v$(VERSION) 2>/dev/null
	git tag "v$(VERSION)"
	git push origin main --tags
	@echo "$(GREEN)Tag v$(VERSION) pushed — GitHub Actions will build the release$(NC)"

