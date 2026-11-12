using System.Linq;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;

// Captures a top-level window's client content losslessly (PrintWindow with PW_RENDERFULLCONTENT),
// so Studio viewport renders can be compared pixel-for-pixel without the MCP's downscaled JPEG.
public static class WinCapture
{
	// The desktop tools/studio/launch.mjs starts Studio on (out of the user's sight and focus)
	public const string STUDIO_DESKTOP = "rur-studio";
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr OpenDesktopW(string name, uint flags, bool inherit, uint access);
	[DllImport("user32.dll")] static extern bool SetThreadDesktop(IntPtr desktop);
	[DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
	[DllImport("user32.dll")] static extern bool IsZoomed(IntPtr hWnd);
	[DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hWnd, int command);
	[DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

	// Runs `work` on the named desktop, in a thread of its own (a thread with windows cannot change desktops);
	// null when the desktop does not exist
	static string OnDesktop(string desktop, Func<string> work)
	{
		IntPtr handle = OpenDesktopW(desktop, 0, false, 0x01FF);
		if (handle == IntPtr.Zero) return null;
		string result = null;
		Exception error = null;
		var thread = new System.Threading.Thread(() =>
		{
			try { if (SetThreadDesktop(handle)) result = work(); }
			catch (Exception e) { error = e; }
		});
		thread.Start();
		thread.Join();
		CloseDesktop(handle);
		if (error != null) throw error;
		return result;
	}

	// Grows the Studio window whose title contains titleMatch by dw x dh physical pixels without activating it,
	// on the user's desktop or on Studio's own: "size=WxH", or null when there is none
	public static string Grow(string titleMatch, int dw, int dh)
	{
		Func<string> grow = () =>
		{
			MakeDpiAware();
			IntPtr found = IntPtr.Zero;
			EnumWindows((hWnd, _) =>
			{
				var title = new StringBuilder(512);
				GetWindowText(hWnd, title, 512);
				string text = title.ToString();
				if (IsWindowVisible(hWnd) && text.IndexOf(titleMatch, StringComparison.OrdinalIgnoreCase) >= 0 && text.EndsWith("Roblox Studio")) found = hWnd;
				return found == IntPtr.Zero;
			}, IntPtr.Zero);
			if (found == IntPtr.Zero) return null;
			RECT rect;
			GetWindowRect(found, out rect);
			if (IsZoomed(found))
			{
				ShowWindow(found, 4); // SW_SHOWNOACTIVATE
				System.Threading.Thread.Sleep(200);
			}
			SetWindowPos(found, IntPtr.Zero, rect.Left, rect.Top, rect.Right - rect.Left + dw, rect.Bottom - rect.Top + dh, 0x0014); // SWP_NOZORDER | SWP_NOACTIVATE
			return String.Format("size={0}x{1}", rect.Right - rect.Left + dw, rect.Bottom - rect.Top + dh);
		};
		return grow() ?? OnDesktop(STUDIO_DESKTOP, grow);
	}

	[StructLayout(LayoutKind.Sequential)]
	public struct RECT { public int Left, Top, Right, Bottom; }

	[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
	[DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
	[DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hWnd, ref Point point);
	[DllImport("user32.dll")] static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
	[DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hWnd);
	[DllImport("user32.dll")] static extern bool IsIconic(IntPtr hWnd);
	[DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
	[DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
	[DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hWnd, StringBuilder text, int count);
	delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

	public static void MakeDpiAware()
	{
		// DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
		SetProcessDpiAwarenessContext(new IntPtr(-4));
	}

	public static string Describe(int processId)
	{
		MakeDpiAware();
		var builder = new StringBuilder();
		EnumWindows((hWnd, _) =>
		{
			uint pid;
			GetWindowThreadProcessId(hWnd, out pid);
			if (pid != processId || !IsWindowVisible(hWnd)) return true;
			RECT rect;
			GetWindowRect(hWnd, out rect);
			var title = new StringBuilder(512);
			GetWindowText(hWnd, title, 512);
			var cls = new StringBuilder(256);
			GetClassName(hWnd, cls, 256);
			builder.AppendLine(String.Format("top {0} class={1} title={2} rect={3},{4},{5},{6} dpi={7} iconic={8}", hWnd, cls, title, rect.Left, rect.Top, rect.Right, rect.Bottom, GetDpiForWindow(hWnd), IsIconic(hWnd)));
			EnumChildWindows(hWnd, (child, __) =>
			{
				RECT childRect;
				GetWindowRect(child, out childRect);
				var childCls = new StringBuilder(256);
				GetClassName(child, childCls, 256);
				var childTitle = new StringBuilder(256);
				GetWindowText(child, childTitle, 256);
				int w = childRect.Right - childRect.Left, h = childRect.Bottom - childRect.Top;
				if (w > 300 && h > 300 && IsWindowVisible(child))
					builder.AppendLine(String.Format("  child {0} class={1} title={2} rect={3},{4} {5}x{6}", child, childCls, childTitle, childRect.Left, childRect.Top, w, h));
				return true;
			}, IntPtr.Zero);
			return true;
		}, IntPtr.Zero);
		return builder.ToString();
	}

	// Finds the Studio viewport: the last visible descendant window of the process's main window
	// whose size matches the viewport's physical pixel size.
	public static long FindViewport(string titleMatch, int width, int height)
	{
		MakeDpiAware();
		long found = 0;
		EnumWindows((hWnd, _) =>
		{
			if (!IsWindowVisible(hWnd)) return true;
			var title = new StringBuilder(512);
			GetWindowText(hWnd, title, 512);
			if (title.ToString().IndexOf(titleMatch, StringComparison.OrdinalIgnoreCase) < 0) return true;
			if (title.ToString().IndexOf("Roblox Studio", StringComparison.OrdinalIgnoreCase) < 0) return true;
			EnumChildWindows(hWnd, (child, __) =>
			{
				RECT rect;
				GetWindowRect(child, out rect);
				int w = rect.Right - rect.Left, h = rect.Bottom - rect.Top;
				if (IsWindowVisible(child) && Math.Abs(w - width) <= 1 && Math.Abs(h - height) <= 1) found = child.ToInt64();
				return true;
			}, IntPtr.Zero);
			return found == 0;
		}, IntPtr.Zero);
		return found;
	}

	// What a failed FindViewport saw: each visible Studio window matching the title, its state and the sizes
	// of its visible child windows (the viewport is one of them)
	public static string Describe(string titleMatch)
	{
		MakeDpiAware();
		var report = new StringBuilder();
		EnumWindows((hWnd, _) =>
		{
			var title = new StringBuilder(512);
			GetWindowText(hWnd, title, 512);
			if (title.ToString().IndexOf("Roblox Studio", StringComparison.OrdinalIgnoreCase) < 0) return true;
			bool matches = title.ToString().IndexOf(titleMatch, StringComparison.OrdinalIgnoreCase) >= 0;
			report.Append((matches ? "* " : "  ") + title + (IsWindowVisible(hWnd) ? "" : " (hidden)") + (IsIconic(hWnd) ? " (minimized)" : "") + ": ");
			var sizes = new List<string>();
			EnumChildWindows(hWnd, (child, __) =>
			{
				RECT rect;
				GetWindowRect(child, out rect);
				int w = rect.Right - rect.Left, h = rect.Bottom - rect.Top;
				if (IsWindowVisible(child) && w >= 200 && h >= 200) sizes.Add(w + "x" + h);
				return true;
			}, IntPtr.Zero);
			report.AppendLine(string.Join(", ", sizes.Distinct()));
			return true;
		}, IntPtr.Zero);
		return report.ToString();
	}

	public static string Capture(long hwndValue, string outPath)
	{
		MakeDpiAware();
		var hWnd = new IntPtr(hwndValue);
		RECT rect;
		GetWindowRect(hWnd, out rect);
		int width = rect.Right - rect.Left, height = rect.Bottom - rect.Top;
		using (var bitmap = new Bitmap(width, height, PixelFormat.Format32bppArgb))
		{
			using (var graphics = Graphics.FromImage(bitmap))
			{
				IntPtr hdc = graphics.GetHdc();
				bool ok = PrintWindow(hWnd, hdc, 2);
				graphics.ReleaseHdc(hdc);
				if (!ok) return "PrintWindow failed";
			}
			bitmap.Save(outPath, ImageFormat.Png);
		}
		return String.Format("{0}x{1} at {2},{3}", width, height, rect.Left, rect.Top);
	}
}
