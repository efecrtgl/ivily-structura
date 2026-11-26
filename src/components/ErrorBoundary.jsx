import { Component } from 'react';

export default class ErrorBoundary extends Component {
  constructor(props) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(error) {
    return { error };
  }
  render() {
    if (this.state.error) {
      return (
        <div className="page">
          <div className="banner danger" role="alert">
            <div>
              <div style={{ fontWeight: 600 }}>This view crashed</div>
              <div className="muted selectable">{String(this.state.error?.message || this.state.error)}</div>
              <button type="button" className="btn sm" style={{ marginTop: 8 }} onClick={() => this.setState({ error: null })}>
                Try again
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
