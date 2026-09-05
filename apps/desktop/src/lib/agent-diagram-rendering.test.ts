import mermaid from 'mermaid';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mermaidConfig, prepareMermaidForRendering } from './agent-diagram-rendering';

interface SvgMeasurementPrototype {
  getBBox?: () => { height: number; width: number; x: number; y: number };
  getComputedTextLength?: () => number;
}

describe('agent diagram rendering', () => {
  const svgPrototype = SVGElement.prototype as unknown as SvgMeasurementPrototype;
  let originalGetBBox: SvgMeasurementPrototype['getBBox'];
  let originalGetComputedTextLength: SvgMeasurementPrototype['getComputedTextLength'];

  beforeEach(() => {
    originalGetBBox = svgPrototype.getBBox;
    originalGetComputedTextLength = svgPrototype.getComputedTextLength;
    Object.defineProperty(svgPrototype, 'getBBox', {
      configurable: true,
      value(this: SVGElement) {
        const text = this.textContent ?? '';
        return { height: 18, width: Math.max(20, text.length * 7), x: 0, y: 0 };
      },
    });
    Object.defineProperty(svgPrototype, 'getComputedTextLength', {
      configurable: true,
      value(this: SVGElement) {
        return Math.max(20, (this.textContent ?? '').length * 7);
      },
    });
  });

  afterEach(() => {
    restoreSvgMethod('getBBox', originalGetBBox);
    restoreSvgMethod('getComputedTextLength', originalGetComputedTextLength);
  });

  it('renders class names as retained SVG text instead of removable foreign objects', async () => {
    mermaid.initialize(mermaidConfig('light'));

    const rendered = await mermaid.render(
      'a3-class-label-regression',
      `classDiagram
        class n0["GUI-Anwendung (Tkinter)"]
        class n1["Task-Manager"]
        n0 --> n1 : listet Aufgaben auf`,
    );
    const document = new DOMParser().parseFromString(rendered.svg, 'image/svg+xml');
    const nodeLabels = Array.from(document.querySelectorAll('.node .label-group')).map((label) =>
      label.textContent?.replace(/\s+/gu, ' ').trim(),
    );

    expect(document.querySelector('.node foreignObject')).toBeNull();
    expect(nodeLabels).toContain('GUI-Anwendung (Tkinter)');
    expect(nodeLabels).toContain('Task-Manager');
  }, 15_000);

  it('renders the persisted task creation flowchart with method-shaped labels', async () => {
    mermaid.initialize(mermaidConfig('light'));

    const rendered = await mermaid.render(
      'a3-task-creation-flowchart-regression',
      prepareMermaidForRendering(`flowchart TD
  n0["TaskFlowManager.add_task(...)"]
  n1["self.storage.save_tasks(tasks)"]
  n2["PluginManager.trigger_task_created(task.to_dict())"]
  n3["AuditLogPlugin.on_task_created(task_data)"]
  n4["AuditLogPlugin._log(message)"]
  n5["Absoluter Pfad zu audit_log.txt (Standardwert)"]
  n0 -->|speichert die Aufgabenliste und löst danach bei aktivierten Plugins das Ereignis aus| n1
  n1 -->|danach, wenn enable_plugins aktiviert ist| n2
  n2 -->|ruft für jedes registrierte Plugin p.on_task_created(task_data) auf| n3
  n3 -->|übergibt eine TASK_CREATED-Nachricht| n4
  n4 -->|öffnet die Logdatei im Anhängemodus und schreibt den Eintrag| n5`),
    );
    const document = new DOMParser().parseFromString(rendered.svg, 'image/svg+xml');

    expect(document.querySelector('parsererror')).toBeNull();
    expect(document.querySelectorAll('.node').length).toBe(6);
    expect(document.querySelectorAll('.edgeLabel').length).toBe(5);
  }, 15_000);

  it('does not rewrite non-flowchart diagrams or already quoted edge labels', () => {
    const sequence = 'sequenceDiagram\n  n0->>n1: call(value)\n';
    const currentFlowchart = 'flowchart TD\n  n0 -->|"call(value)"| n1\n';

    expect(prepareMermaidForRendering(sequence)).toBe(sequence);
    expect(prepareMermaidForRendering(currentFlowchart)).toBe(currentFlowchart);
  });
});

function restoreSvgMethod(
  name: keyof SvgMeasurementPrototype,
  original: SvgMeasurementPrototype[keyof SvgMeasurementPrototype],
): void {
  const svgPrototype = SVGElement.prototype as unknown as SvgMeasurementPrototype;
  if (original) {
    Object.defineProperty(svgPrototype, name, { configurable: true, value: original });
  } else {
    delete svgPrototype[name];
  }
}
