<script lang="ts">
  interface Props {
    addLabel: string;
    itemLabel: string;
    legend: string;
    values?: string[];
  }

  let { addLabel, itemLabel, legend, values = $bindable([]) }: Props = $props();

  function addItem(): void {
    if (values.length < 64) values = [...values, ''];
  }

  function removeItem(index: number): void {
    values = values.filter((_, itemIndex) => itemIndex !== index);
  }

  function updateItem(index: number, event: Event): void {
    const target = event.currentTarget;
    if (!(target instanceof HTMLTextAreaElement)) return;
    values = values.map((value, itemIndex) => (itemIndex === index ? target.value : value));
  }
</script>

<fieldset class="goal-text-list">
  <legend>{legend}</legend>
  {#each values as value, index (index)}
    <div class="goal-text-item">
      <label>
        <span>{itemLabel} {index + 1}</span>
        <textarea maxlength="4096" rows="2" {value} oninput={(event) => updateItem(index, event)}
        ></textarea>
      </label>
      <button
        type="button"
        aria-label={`${itemLabel} ${index + 1} entfernen`}
        onclick={() => removeItem(index)}
      >
        Entfernen
      </button>
    </div>
  {/each}
  <button type="button" disabled={values.length >= 64} onclick={addItem}>{addLabel}</button>
</fieldset>

<style>
  .goal-text-list {
    border: 1px solid var(--line, #d8d9df);
    border-radius: 0.75rem;
    display: grid;
    gap: 0.75rem;
    margin: 0;
    padding: 0.9rem;
  }

  legend {
    font-weight: 700;
    padding: 0 0.35rem;
  }

  .goal-text-item {
    align-items: end;
    display: grid;
    gap: 0.6rem;
    grid-template-columns: minmax(0, 1fr) auto;
  }

  label,
  label span {
    display: grid;
    gap: 0.35rem;
  }

  textarea {
    box-sizing: border-box;
    font: inherit;
    min-height: 3.6rem;
    resize: vertical;
    width: 100%;
  }

  button {
    min-height: 2.4rem;
  }

  @media (max-width: 720px) {
    .goal-text-item {
      align-items: stretch;
      grid-template-columns: 1fr;
    }
  }
</style>
