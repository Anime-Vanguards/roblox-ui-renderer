using System;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Timed capture series of a window region (PrintWindow, lossless), for animations such as the TextBox
// caret blink: frames are kept in memory and written after the series, with their capture times in ms
// since the series started (the first frame is taken right after `startDelayMs`).
public static class WinSeries
{
	[StructLayout(LayoutKind.Sequential)]
	public struct RECT { public int Left, Top, Right, Bottom; }

	[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
	[DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
	[DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);

	public static string Capture(long hwndValue, int count, int intervalMs, int cropX, int cropY, int cropW, int cropH, string outPrefix)
	{
		SetProcessDpiAwarenessContext(new IntPtr(-4));
		var hWnd = new IntPtr(hwndValue);
		RECT rect;
		GetWindowRect(hWnd, out rect);
		int width = rect.Right - rect.Left, height = rect.Bottom - rect.Top;
		var frames = new Bitmap[count];
		var times = new double[count];
		var clock = Stopwatch.StartNew();
		using (var full = new Bitmap(width, height, PixelFormat.Format32bppArgb))
		{
			for (int i = 0; i < count; i++)
			{
				double target = (double)i * intervalMs;
				while (clock.Elapsed.TotalMilliseconds < target) Thread.Sleep(1);
				using (var graphics = Graphics.FromImage(full))
				{
					IntPtr hdc = graphics.GetHdc();
					times[i] = clock.Elapsed.TotalMilliseconds;
					PrintWindow(hWnd, hdc, 2);
					graphics.ReleaseHdc(hdc);
				}
				frames[i] = full.Clone(new Rectangle(cropX, cropY, cropW, cropH), PixelFormat.Format32bppArgb);
			}
		}
		var json = new StringBuilder("[");
		for (int i = 0; i < count; i++)
		{
			frames[i].Save(outPrefix + i.ToString("D3") + ".png", ImageFormat.Png);
			frames[i].Dispose();
			json.Append(i == 0 ? "" : ",").Append(times[i].ToString("F2", System.Globalization.CultureInfo.InvariantCulture));
		}
		json.Append("]");
		File.WriteAllText(outPrefix + "times.json", json.ToString());
		return json.ToString();
	}
}
