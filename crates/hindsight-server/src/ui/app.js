// Hindsight Web UI - JavaScript Client

class HindsightApp {
    constructor() {
        this.traces = [];
        this.filteredTraces = [];
        this.selectedTrace = null;
        this.init();
    }

    init() {
        this.setupEventListeners();
        this.loadTraces();
        this.startAutoRefresh();
    }

    setupEventListeners() {
        // Refresh button
        document.getElementById('refreshBtn').addEventListener('click', () => {
            this.loadTraces();
        });

        // Filters
        document.getElementById('serviceFilter').addEventListener('change', () => this.applyFilters());
        document.getElementById('typeFilter').addEventListener('change', () => this.applyFilters());
        document.getElementById('minDuration').addEventListener('input', () => this.applyFilters());
        document.getElementById('searchQuery').addEventListener('input', () => this.applyFilters());
    }

    async loadTraces() {
        try {
            const response = await fetch('/api/traces');
            const data = await response.json();

            if (data.error) {
                console.error('Error loading traces:', data.error);
                this.updateConnectionStatus(false);
                return;
            }

            this.traces = data.traces || [];
            this.applyFilters();
            this.updateConnectionStatus(true);
            this.updateStats();
        } catch (error) {
            console.error('Failed to load traces:', error);
            this.updateConnectionStatus(false);
        }
    }

    applyFilters() {
        const serviceFilter = document.getElementById('serviceFilter').value;
        const typeFilter = document.getElementById('typeFilter').value;
        const minDuration = parseInt(document.getElementById('minDuration').value) || 0;
        const searchQuery = document.getElementById('searchQuery').value.toLowerCase();

        this.filteredTraces = this.traces.filter(trace => {
            // Service filter
            if (serviceFilter && trace.service_name !== serviceFilter) return false;

            // Type filter
            if (typeFilter && trace.trace_type !== typeFilter) return false;

            // Duration filter (convert nanos to ms)
            const durationMs = trace.duration_nanos / 1_000_000;
            if (durationMs < minDuration) return false;

            // Search filter
            if (searchQuery) {
                const searchText = `${trace.root_span_name} ${trace.service_name} ${trace.trace_id}`.toLowerCase();
                if (!searchText.includes(searchQuery)) return false;
            }

            return true;
        });

        this.renderTraces();
        this.updateServiceFilter();
        this.updateStats();
    }

    renderTraces() {
        const traceList = document.getElementById('traceList');

        if (this.filteredTraces.length === 0) {
            traceList.innerHTML = `
                <div class="empty-state">
                    <div class="empty-state-icon">📭</div>
                    <div class="empty-state-title">No traces found</div>
                    <div class="empty-state-text">
                        ${this.traces.length === 0
                            ? 'Send some traces from your application to see them here.'
                            : 'Try adjusting your filters to see more results.'}
                    </div>
                </div>
            `;
            return;
        }

        const html = this.filteredTraces.map(trace => this.renderTraceCard(trace)).join('');
        traceList.innerHTML = html;

        // Add click handlers
        traceList.querySelectorAll('.trace-card').forEach((card, index) => {
            card.addEventListener('click', () => {
                this.showTraceDetail(this.filteredTraces[index]);
            });
        });
    }

    renderTraceCard(trace) {
        const durationMs = (trace.duration_nanos / 1_000_000).toFixed(2);
        const typeClass = `type-${trace.trace_type.toLowerCase()}`;

        return `
            <div class="trace-card" data-trace-id="${trace.trace_id}">
                <div class="trace-header">
                    <div class="trace-name">${this.escapeHtml(trace.root_span_name)}</div>
                    <div class="trace-duration">${durationMs}ms</div>
                </div>
                <div class="trace-meta">
                    <span>🏷️ ${this.escapeHtml(trace.service_name)}</span>
                    <span>📊 ${trace.span_count} spans</span>
                    ${trace.error_count > 0 ? `<span style="color: #ef4444;">⚠️ ${trace.error_count} errors</span>` : ''}
                    <span class="trace-type-badge ${typeClass}">${trace.trace_type}</span>
                </div>
            </div>
        `;
    }

    async showTraceDetail(traceSummary) {
        try {
            const response = await fetch(`/api/traces/${traceSummary.trace_id}`);
            const data = await response.json();

            if (data.error) {
                console.error('Error loading trace detail:', data.error);
                return;
            }

            this.renderTraceDetail(data.trace, traceSummary);
        } catch (error) {
            console.error('Failed to load trace detail:', error);
        }
    }

    renderTraceDetail(trace, summary) {
        const durationMs = (summary.duration_nanos / 1_000_000).toFixed(2);
        const startTime = new Date(summary.start_time_nanos / 1_000_000).toLocaleString();

        const traceList = document.getElementById('traceList');
        traceList.innerHTML = `
            <div class="trace-detail">
                <div style="margin-bottom: 1rem;">
                    <button class="btn" onclick="app.loadTraces()" style="background: #475569;">← Back to List</button>
                </div>

                <div class="detail-header">
                    <div class="detail-title">${this.escapeHtml(summary.root_span_name)}</div>
                    <div class="detail-meta">
                        <span>🏷️ Service: ${this.escapeHtml(summary.service_name)}</span>
                        <span>⏱️ Duration: ${durationMs}ms</span>
                        <span>📊 Spans: ${summary.span_count}</span>
                        <span>🕐 Started: ${startTime}</span>
                    </div>
                </div>

                <div class="waterfall">
                    <div class="waterfall-header">Span Waterfall</div>
                    ${this.renderWaterfall(trace.spans, summary.start_time_nanos, summary.duration_nanos)}
                </div>

                <div class="waterfall" style="margin-top: 1.5rem;">
                    <div class="waterfall-header">Span Details</div>
                    ${this.renderSpanDetails(trace.spans)}
                </div>
            </div>
        `;
    }

    renderWaterfall(spans, traceStart, traceDuration) {
        // Build span tree
        const spanMap = new Map();
        const rootSpans = [];

        spans.forEach(span => {
            spanMap.set(span.span_id, { ...span, children: [] });
        });

        spans.forEach(span => {
            const node = spanMap.get(span.span_id);
            if (span.parent_span_id) {
                const parent = spanMap.get(span.parent_span_id);
                if (parent) {
                    parent.children.push(node);
                } else {
                    rootSpans.push(node);
                }
            } else {
                rootSpans.push(node);
            }
        });

        const html = [];
        const renderSpan = (span, depth = 0) => {
            const duration = span.duration_nanos || 0;
            const durationMs = (duration / 1_000_000).toFixed(2);
            const startOffset = span.start_time_nanos - traceStart;
            const leftPercent = (startOffset / traceDuration) * 100;
            const widthPercent = (duration / traceDuration) * 100;

            const indent = depth * 20;

            html.push(`
                <div class="span-row">
                    <div class="span-bar" style="margin-left: ${indent}px; width: calc(${widthPercent}% - ${indent}px); left: ${leftPercent}%;">
                        <span class="span-name">${this.escapeHtml(span.name)}</span>
                        <span class="span-duration">${durationMs}ms</span>
                    </div>
                </div>
            `);

            span.children.forEach(child => renderSpan(child, depth + 1));
        };

        rootSpans.forEach(span => renderSpan(span));

        return html.join('');
    }

    renderSpanDetails(spans) {
        return spans.map(span => {
            const durationMs = span.duration_nanos ? (span.duration_nanos / 1_000_000).toFixed(2) : 'N/A';
            const attributes = span.attributes.map(attr => {
                return `<li><strong>${this.escapeHtml(attr.key)}:</strong> ${this.escapeHtml(String(attr.value))}</li>`;
            }).join('');

            return `
                <div style="margin-bottom: 1.5rem; padding: 1rem; background: #0f172a; border-radius: 0.375rem;">
                    <div style="font-weight: 600; margin-bottom: 0.5rem;">${this.escapeHtml(span.name)}</div>
                    <div style="font-size: 0.875rem; color: #94a3b8;">
                        Duration: ${durationMs}ms | Service: ${this.escapeHtml(span.service_name)}
                    </div>
                    ${attributes ? `
                        <div style="margin-top: 0.75rem;">
                            <div style="font-size: 0.875rem; font-weight: 600; margin-bottom: 0.25rem;">Attributes:</div>
                            <ul style="margin-left: 1.5rem; font-size: 0.875rem; color: #cbd5e1;">
                                ${attributes}
                            </ul>
                        </div>
                    ` : ''}
                </div>
            `;
        }).join('');
    }

    updateServiceFilter() {
        const select = document.getElementById('serviceFilter');
        const currentValue = select.value;

        // Get unique services
        const services = [...new Set(this.traces.map(t => t.service_name))].sort();

        // Rebuild options
        select.innerHTML = '<option value="">All Services</option>';
        services.forEach(service => {
            const option = document.createElement('option');
            option.value = service;
            option.textContent = service;
            select.appendChild(option);
        });

        // Restore selection if still valid
        if (services.includes(currentValue)) {
            select.value = currentValue;
        }
    }

    updateStats() {
        document.getElementById('totalTraces').textContent = this.traces.length;
        document.getElementById('shownTraces').textContent = this.filteredTraces.length;
    }

    updateConnectionStatus(connected) {
        const status = document.getElementById('connectionStatus');
        if (connected) {
            status.textContent = 'Connected';
            status.parentElement.querySelector('.status-dot').style.background = '#10b981';
        } else {
            status.textContent = 'Disconnected';
            status.parentElement.querySelector('.status-dot').style.background = '#ef4444';
        }
    }

    startAutoRefresh() {
        // Refresh every 5 seconds
        setInterval(() => {
            this.loadTraces();
        }, 5000);
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// Initialize app when DOM is ready
let app;
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        app = new HindsightApp();
    });
} else {
    app = new HindsightApp();
}
