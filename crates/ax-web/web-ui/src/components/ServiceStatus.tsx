import './ServiceStatus.css';
export type ServiceState = 'running' | 'warning' | 'error' | 'stopped';
export default function ServiceStatus({ state, label, active = false }: { state: ServiceState; label: string; active?: boolean }) {
  return <span role="status" className={`service-status service-status--${state}${active ? ' is-active' : ''}`}><span className="service-status-dot" aria-hidden="true" />{label}</span>;
}
