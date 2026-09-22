<svelte:head>
  <title>Onboarding · trans-kun</title>
</svelte:head>

<section class="route-screen onboarding-screen" aria-labelledby="onboarding-title">
  <div class="route-kicker">Bước 1 · Thiết lập</div>
  <h1 id="onboarding-title">Chào mừng đến trans-kun</h1>
  <p class="route-lede">
    Chọn ngôn ngữ, xem cách dữ liệu được gửi và thêm API key ở các bước tiếp theo.
  </p>

  <div class="stepper" aria-label="Tiến độ thiết lập">
    <span class="step step-current" aria-current="step"><span>1</span> Ngôn ngữ</span>
    <span class="step"><span>2</span> Đồng ý</span>
    <span class="step"><span>3</span> API key</span>
  </div>

  <div class="placeholder-card">
    <p class="placeholder-title">Luồng onboarding đang sẵn sàng</p>
    <p class="placeholder-copy">
      Màn hình đầy đủ sẽ được thêm ở các story onboarding tiếp theo. Bạn vẫn có thể vào
      Trang chủ để xem app shell.
    </p>
  </div>
</section>

<style>
  .route-screen {
    max-width: 760px;
    margin: 0 auto;
    padding: var(--space-8);
  }

  .onboarding-screen {
    max-width: 680px;
  }

  .route-kicker {
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  h1 {
    margin: var(--space-2) 0 var(--space-3);
    font-size: var(--text-h1-size);
    line-height: 1.3;
  }

  .route-lede,
  .placeholder-copy {
    margin: 0;
    color: var(--color-text-secondary);
  }

  .stepper {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: var(--space-6) 0;
  }

  .step {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    color: var(--color-text-muted);
    font-size: var(--text-label-size);
  }

  .step span {
    display: grid;
    width: 20px;
    height: 20px;
    place-items: center;
    border-radius: var(--radius-full);
    background: var(--color-surface-sunken);
    font-size: var(--text-help-size);
    font-weight: 600;
  }

  .step-current {
    border-color: var(--color-accent-border);
    background: var(--color-accent-soft);
    color: var(--color-text);
  }

  .step-current span {
    background: var(--color-accent);
    color: var(--color-surface);
  }

  .placeholder-card {
    padding: var(--space-6);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    background: var(--color-surface);
  }

  .placeholder-title {
    margin: 0 0 var(--space-2);
    font-weight: 600;
  }
</style>
