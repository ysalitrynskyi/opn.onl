#!/bin/bash

# Run all tests script for opn.onl project
# Usage: ./scripts/run-all-tests.sh [--backend|--frontend|--e2e|--all]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
print_header() {
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# Run backend tests
run_backend_tests() {
    print_header "Running Backend Tests (Rust)"
    
    cd "$PROJECT_ROOT/backend"
    
    # Run cargo tests
    if cargo test --all-features 2>&1; then
        print_success "Backend tests passed"
        return 0
    else
        print_error "Backend tests failed"
        return 1
    fi
}

# Run frontend unit tests
run_frontend_tests() {
    print_header "Running Frontend Unit Tests (Vitest)"
    
    cd "$PROJECT_ROOT/frontend"
    
    # Check if node_modules exists
    if [ ! -d "node_modules" ]; then
        print_warning "Installing frontend dependencies..."
        npm install
    fi
    
    # Run vitest
    if npm run test -- --run 2>&1; then
        print_success "Frontend unit tests passed"
        return 0
    else
        print_error "Frontend unit tests failed"
        return 1
    fi
}

# Run E2E tests
run_e2e_tests() {
    print_header "Running E2E Tests (Playwright)"
    
    cd "$PROJECT_ROOT/frontend"
    
    # Check if playwright is installed
    if [ ! -d "node_modules" ]; then
        print_warning "Installing frontend dependencies..."
        npm install
    fi
    
    # Install playwright browsers if needed
    npx playwright install --with-deps 2>/dev/null || true
    
    # Run playwright tests
    if npx playwright test 2>&1; then
        print_success "E2E tests passed"
        return 0
    else
        print_error "E2E tests failed"
        return 1
    fi
}

# Run all tests
run_all_tests() {
    local exit_code=0
    
    run_backend_tests || exit_code=1
    run_frontend_tests || exit_code=1
    run_e2e_tests || exit_code=1
    
    echo ""
    if [ $exit_code -eq 0 ]; then
        print_header "All Tests Passed! ✓"
    else
        print_header "Some Tests Failed ✗"
    fi
    
    return $exit_code
}

# Show usage
show_usage() {
    echo "Usage: $0 [option]"
    echo ""
    echo "Options:"
    echo "  --backend    Run backend (Rust) tests only"
    echo "  --frontend   Run frontend (Vitest) unit tests only"
    echo "  --e2e        Run E2E (Playwright) tests only"
    echo "  --all        Run all tests (default)"
    echo "  --help       Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --backend     # Run only backend tests"
    echo "  $0 --frontend    # Run only frontend tests"
    echo "  $0               # Run all tests"
}

# Main
case "${1:-all}" in
    --backend)
        run_backend_tests
        ;;
    --frontend)
        run_frontend_tests
        ;;
    --e2e)
        run_e2e_tests
        ;;
    --all|all)
        run_all_tests
        ;;
    --help|-h)
        show_usage
        ;;
    *)
        echo "Unknown option: $1"
        show_usage
        exit 1
        ;;
esac
