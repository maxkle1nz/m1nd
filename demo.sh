#!/usr/bin/env bash
# ============================================================================
# m1nd demo — see the value in 3 minutes
#
# Usage:
#   ./demo.sh                    # uses a bundled demo repo
#   ./demo.sh /path/to/your/repo # uses your own codebase
#
# Requirements: m1nd-mcp binary (cargo build --release, or download from
# https://github.com/maxkle1nz/m1nd/releases)
# ============================================================================

set -euo pipefail

CYAN='\033[36m'
YELLOW='\033[33m'
MAGENTA='\033[35m'
BLUE='\033[34m'
GREEN='\033[32m'
DIM='\033[2m'
BOLD='\033[1m'
RESET='\033[0m'

BINARY="${M1ND_BINARY:-./target/release/m1nd-mcp}"
DEMO_REPO="${1:-}"
AGENT_ID="demo"
REQUEST_ID=0

# --- helpers ----------------------------------------------------------------

banner() {
  echo ""
  echo -e "${CYAN}⍌${YELLOW}⍐${MAGENTA}⍂${BLUE}𝔻${RESET} ${GREEN}⟁${RESET}  ${BOLD}m1nd demo${RESET}"
  echo -e "${DIM}see the value in 3 minutes${RESET}"
  echo ""
}

check_binary() {
  if [[ ! -x "$BINARY" ]]; then
    echo -e "${MAGENTA}Binary not found at $BINARY${RESET}"
    echo ""
    echo "Build it:"
    echo "  cargo build --release"
    echo ""
    echo "Or download from:"
    echo "  https://github.com/maxkle1nz/m1nd/releases"
    echo ""
    echo "Or set M1ND_BINARY=/path/to/m1nd-mcp"
    exit 1
  fi
}

# Send a JSON-RPC request to m1nd and extract the result
call_m1nd() {
  local tool="$1"
  local args="$2"
  REQUEST_ID=$((REQUEST_ID + 1))

  local request="{\"jsonrpc\":\"2.0\",\"id\":${REQUEST_ID},\"method\":\"tools/call\",\"params\":{\"name\":\"${tool}\",\"arguments\":${args}}}"

  echo "$request" | "$BINARY" 2>/dev/null | python3 -c "
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
        if 'result' in msg:
            content = msg['result'].get('content', [])
            for c in content:
                if c.get('type') == 'text':
                    # try to parse as JSON for pretty print
                    try:
                        data = json.loads(c['text'])
                        print(json.dumps(data, indent=2))
                    except:
                        print(c['text'])
            break
    except:
        pass
"
}

step() {
  local num="$1"
  local glyph="$2"
  local color="$3"
  local title="$4"
  local desc="$5"
  echo ""
  echo -e "${BOLD}${color}${glyph} Step ${num}: ${title}${RESET}"
  echo -e "${DIM}${desc}${RESET}"
  echo ""
}

pause() {
  echo ""
  echo -e "${DIM}press enter to continue...${RESET}"
  read -r
}

# --- setup demo repo --------------------------------------------------------

setup_demo_repo() {
  if [[ -n "$DEMO_REPO" ]]; then
    echo -e "Using your repo: ${BOLD}$DEMO_REPO${RESET}"
    return
  fi

  DEMO_REPO=$(mktemp -d)
  echo -e "Creating demo repo at ${DIM}$DEMO_REPO${RESET}"

  # A small but realistic Python project with interesting structure
  mkdir -p "$DEMO_REPO/src" "$DEMO_REPO/tests"

  cat > "$DEMO_REPO/src/server.py" << 'PYEOF'
from src.router import Router
from src.database import DatabasePool
from src.auth import AuthMiddleware
from src.cache import CacheLayer

class Server:
    def __init__(self, config):
        self.db = DatabasePool(config.db_url, max_connections=10)
        self.router = Router()
        self.auth = AuthMiddleware(config.secret_key)
        self.cache = CacheLayer(ttl=300)

    def handle_request(self, request):
        if not self.auth.verify(request):
            return {"error": "unauthorized"}
        cached = self.cache.get(request.path)
        if cached:
            return cached
        response = self.router.dispatch(request)
        self.cache.set(request.path, response)
        return response

    def shutdown(self):
        self.db.close_all()
        self.cache.flush()
PYEOF

  cat > "$DEMO_REPO/src/router.py" << 'PYEOF'
from src.handlers import UserHandler, OrderHandler, SearchHandler

class Router:
    def __init__(self):
        self.routes = {
            "/users": UserHandler(),
            "/orders": OrderHandler(),
            "/search": SearchHandler(),
        }

    def dispatch(self, request):
        handler = self.routes.get(request.path)
        if not handler:
            return {"error": "not found"}
        return handler.handle(request)
PYEOF

  cat > "$DEMO_REPO/src/handlers.py" << 'PYEOF'
from src.database import DatabasePool
from src.models import User, Order
from src.search_engine import SearchEngine
from src.notifications import NotificationService

class UserHandler:
    def handle(self, request):
        db = DatabasePool.instance()
        user = db.query(User, request.params.get("id"))
        return {"user": user.to_dict()}

class OrderHandler:
    def handle(self, request):
        db = DatabasePool.instance()
        order = db.query(Order, request.params.get("id"))
        NotificationService.send("order_viewed", order.id)
        return {"order": order.to_dict()}

class SearchHandler:
    def handle(self, request):
        engine = SearchEngine()
        results = engine.search(request.params.get("q"))
        return {"results": results}
PYEOF

  cat > "$DEMO_REPO/src/database.py" << 'PYEOF'
import threading

class DatabasePool:
    _instance = None
    _lock = threading.Lock()

    def __init__(self, url, max_connections=5):
        self.url = url
        self.max_connections = max_connections
        self.connections = []
        self._active = 0

    @classmethod
    def instance(cls):
        if cls._instance is None:
            raise RuntimeError("DatabasePool not initialized")
        return cls._instance

    def acquire(self):
        with self._lock:
            if self._active >= self.max_connections:
                raise RuntimeError("Pool exhausted")
            self._active += 1
            return self.connections[self._active - 1]

    def release(self, conn):
        with self._lock:
            self._active -= 1

    def query(self, model, id):
        conn = self.acquire()
        try:
            return conn.execute(f"SELECT * FROM {model.__table__} WHERE id = ?", id)
        finally:
            self.release(conn)

    def close_all(self):
        for conn in self.connections:
            conn.close()
PYEOF

  cat > "$DEMO_REPO/src/auth.py" << 'PYEOF'
import hashlib
import time

class AuthMiddleware:
    def __init__(self, secret_key):
        self.secret_key = secret_key
        self.token_cache = {}

    def verify(self, request):
        token = request.headers.get("Authorization")
        if not token:
            return False
        if token in self.token_cache:
            return self.token_cache[token]
        valid = self._validate_token(token)
        self.token_cache[token] = valid
        return valid

    def _validate_token(self, token):
        parts = token.split(".")
        if len(parts) != 3:
            return False
        payload, signature = parts[1], parts[2]
        expected = hashlib.sha256(f"{payload}{self.secret_key}".encode()).hexdigest()[:16]
        return signature == expected
PYEOF

  cat > "$DEMO_REPO/src/cache.py" << 'PYEOF'
import time

class CacheLayer:
    def __init__(self, ttl=60):
        self.ttl = ttl
        self.store = {}

    def get(self, key):
        if key in self.store:
            value, expires = self.store[key]
            if time.time() < expires:
                return value
            del self.store[key]
        return None

    def set(self, key, value):
        self.store[key] = (value, time.time() + self.ttl)

    def flush(self):
        self.store.clear()

    def stats(self):
        now = time.time()
        active = sum(1 for _, (_, exp) in self.store.items() if exp > now)
        return {"total": len(self.store), "active": active}
PYEOF

  cat > "$DEMO_REPO/src/models.py" << 'PYEOF'
class User:
    __table__ = "users"
    def __init__(self, id, name, email):
        self.id = id
        self.name = name
        self.email = email
    def to_dict(self):
        return {"id": self.id, "name": self.name, "email": self.email}

class Order:
    __table__ = "orders"
    def __init__(self, id, user_id, total, status):
        self.id = id
        self.user_id = user_id
        self.total = total
        self.status = status
    def to_dict(self):
        return {"id": self.id, "user_id": self.user_id, "total": self.total, "status": self.status}
PYEOF

  cat > "$DEMO_REPO/src/search_engine.py" << 'PYEOF'
from src.database import DatabasePool

class SearchEngine:
    def __init__(self):
        self.db = DatabasePool.instance()
        self.index = {}

    def search(self, query):
        if query in self.index:
            return self.index[query]
        results = self.db.query_raw(f"SELECT * FROM search_index WHERE text LIKE '%{query}%'")
        self.index[query] = results
        return results

    def rebuild_index(self):
        self.index.clear()
PYEOF

  cat > "$DEMO_REPO/src/notifications.py" << 'PYEOF'
import logging

class NotificationService:
    _handlers = {}

    @classmethod
    def register(cls, event, handler):
        cls._handlers.setdefault(event, []).append(handler)

    @classmethod
    def send(cls, event, data):
        handlers = cls._handlers.get(event, [])
        for handler in handlers:
            try:
                handler(data)
            except Exception as e:
                logging.error(f"Notification handler failed: {e}")
PYEOF

  cat > "$DEMO_REPO/src/__init__.py" << 'PYEOF'
PYEOF

  cat > "$DEMO_REPO/tests/test_server.py" << 'PYEOF'
from src.server import Server
from src.database import DatabasePool

def test_server_init():
    config = type("Config", (), {"db_url": "sqlite://test", "secret_key": "test"})
    server = Server(config)
    assert server.db is not None
    assert server.router is not None

def test_unauthorized_request():
    config = type("Config", (), {"db_url": "sqlite://test", "secret_key": "test"})
    server = Server(config)
    request = type("Request", (), {"headers": {}, "path": "/users", "params": {}})
    response = server.handle_request(request)
    assert response == {"error": "unauthorized"}
PYEOF

  echo -e "${GREEN}Demo repo created: 9 Python files${RESET}"
}

cleanup() {
  if [[ -z "${1:-}" && -n "$DEMO_REPO" && "$DEMO_REPO" == /tmp/* ]]; then
    rm -rf "$DEMO_REPO"
  fi
}

# --- main -------------------------------------------------------------------

banner
check_binary

setup_demo_repo "$@"

# Initialize m1nd server for the demo
export M1ND_GRAPH_SOURCE="$DEMO_REPO/graph_snapshot.json"
export M1ND_PLASTICITY_STATE="$DEMO_REPO/plasticity_state.json"

step 1 "⍌" "$CYAN" "Ingest" "Load the codebase into the graph"
echo -e "${DIM}m1nd.ingest — scanning $DEMO_REPO${RESET}"
call_m1nd "m1nd.ingest" "{\"agent_id\":\"$AGENT_ID\",\"source\":\"filesystem\",\"path\":\"$DEMO_REPO\"}"
pause

step 2 "⍌" "$CYAN" "Activate" "\"What's related to database connections?\""
echo -e "${DIM}m1nd.activate — spreading activation through the graph${RESET}"
call_m1nd "m1nd.activate" "{\"agent_id\":\"$AGENT_ID\",\"query\":\"database connection pool\",\"top_k\":5}"
pause

step 3 "𝔻" "$BLUE" "Impact" "\"What breaks if I change database.py?\""
echo -e "${DIM}m1nd.impact — blast radius analysis${RESET}"
call_m1nd "m1nd.impact" "{\"agent_id\":\"$AGENT_ID\",\"node_id\":\"file::src/database.py\",\"depth\":3}"
pause

step 4 "⍂" "$MAGENTA" "Missing" "\"What's missing around authentication?\""
echo -e "${DIM}m1nd.missing — structural hole detection (Burt's network theory)${RESET}"
call_m1nd "m1nd.missing" "{\"agent_id\":\"$AGENT_ID\",\"query\":\"authentication security\"}"
pause

step 5 "𝔻" "$BLUE" "Counterfactual" "\"What happens if I delete notifications.py?\""
echo -e "${DIM}m1nd.counterfactual — simulate removal without touching code${RESET}"
call_m1nd "m1nd.counterfactual" "{\"agent_id\":\"$AGENT_ID\",\"node_ids\":[\"file::src/notifications.py\"]}"
pause

echo ""
echo -e "${BOLD}${GREEN}Done.${RESET} Five queries, zero LLM tokens, zero API calls."
echo ""
echo -e "Next steps:"
echo -e "  ${CYAN}⍌${RESET} m1nd.learn    — teach the graph which results were useful"
echo -e "  ${YELLOW}⍐${RESET} m1nd.why      — trace the path between any two modules"
echo -e "  ${MAGENTA}⍂${RESET} m1nd.scan     — full structural analysis"
echo -e "  ${BLUE}𝔻${RESET} m1nd.predict  — what else needs to change after an edit"
echo -e "  ${GREEN}⟁${RESET} m1nd.resonate — find resonance patterns in the graph"
echo ""
echo -e "Run against ${BOLD}your own repo${RESET}:"
echo -e "  ./demo.sh /path/to/your/project"
echo ""
echo -e "${CYAN}⍌${YELLOW}⍐${MAGENTA}⍂${BLUE}𝔻${RESET} ${GREEN}⟁${RESET}  ${DIM}https://m1nd.world${RESET}"

trap cleanup EXIT
