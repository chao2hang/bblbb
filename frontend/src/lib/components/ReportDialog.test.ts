import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import ReportDialog from './ReportDialog.svelte';
import * as client from '$lib/api/client';
import * as toast from '$lib/ui/toast';

describe('ReportDialog 举报弹窗组件', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('open=false 时不渲染弹窗内容', () => {
    render(ReportDialog, { open: false, targetType: 'post', targetId: 'p-1' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('open=true 时渲染弹窗与表单控件', () => {
    render(ReportDialog, {
      open: true,
      targetType: 'post',
      targetId: 'post-1234567890',
      targetTitle: '违规帖子标题'
    });

    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(screen.getByText('举报帖子')).toBeInTheDocument();
    expect(screen.getByText('违规帖子标题')).toBeInTheDocument();
    expect(screen.getByText('帖子')).toBeInTheDocument();
    expect(screen.getByLabelText('举报原因')).toBeInTheDocument();
    expect(screen.getByLabelText(/补充说明/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '提交举报' })).toBeInTheDocument();
  });

  it('提交成功后调用 createReport、显示 Toast 并关闭弹窗', async () => {
    const createReportSpy = vi.spyOn(client, 'createReport').mockResolvedValueOnce({
      id: 'report-1',
      status: 'submitted'
    });
    const toastSpy = vi.spyOn(toast, 'show');
    const onsuccess = vi.fn();
    const onclose = vi.fn();

    render(ReportDialog, {
      open: true,
      targetType: 'post',
      targetId: 'p-1',
      onsuccess,
      onclose
    });

    const reasonSelect = screen.getByLabelText('举报原因');
    await userEvent.selectOptions(reasonSelect, 'harassment');

    const detailInput = screen.getByLabelText(/补充说明/);
    await userEvent.type(detailInput, '包含恶意侮辱');

    const submitBtn = screen.getByRole('button', { name: '提交举报' });
    await userEvent.click(submitBtn);

    await waitFor(() => {
      expect(createReportSpy).toHaveBeenCalledWith(expect.anything(), {
        target_type: 'post',
        target_id: 'p-1',
        reason: 'harassment',
        detail: '包含恶意侮辱'
      });
    });

    await waitFor(() => {
      expect(toastSpy).toHaveBeenCalledWith('举报已提交，我们会尽快核实处理', 'success');
      expect(onsuccess).toHaveBeenCalledWith({ id: 'report-1', status: 'submitted' });
      expect(onclose).toHaveBeenCalled();
    });
  });

  it('提交失败时弹窗内展示错误信息且不关闭弹窗', async () => {
    vi.spyOn(client, 'createReport').mockRejectedValueOnce({
      status: 422,
      code: 'cannot_report_own_content',
      detail: '不能举报自己的内容'
    });

    render(ReportDialog, {
      open: true,
      targetType: 'comment',
      targetId: 'c-1'
    });

    const submitBtn = screen.getByRole('button', { name: '提交举报' });
    await userEvent.click(submitBtn);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
  });
});
