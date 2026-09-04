import mermaid from 'mermaid';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mermaidConfig } from './agent-diagram-rendering';

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
