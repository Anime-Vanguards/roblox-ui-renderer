// Starts Roblox Studio without taking the user's keyboard focus, and keeps it from taking it later
// (tools/studio/launch.mjs; loaded by launch.ps1 through Add-Type). Measured on Windows 11, 2026-11-12:
//
// - CreateProcess with STARTUPINFO.wShowWindow = SW_SHOWNOACTIVATE: the splash and the main window open without
//   becoming the foreground window.
// - The main window is moved fully off the visible desktop (above the primary monitor, so it keeps that monitor's
//   display scale) at the bottom of the z-order, without activation. Studio keeps rendering there and PrintWindow
//   captures it at the right scale. (A desktop of its own, CreateDesktop, keeps windows out of focus entirely, but
//   Studio's viewport is not drawn there: every capture came out empty.)
// - Studio draws its own cursor at the viewport's centre until the mouse has been over the viewport; a posted
//   WM_MOUSEMOVE (no real input, the cursor does not move) puts it in the viewport's bottom-right corner.
// - A guard watches the foreground for a while: a window of the Studio process that became the foreground anyway
//   (a dialog) is minimized, so Windows activates the window below it, then shown again without activation.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class StudioLaunch
{
	[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
	struct STARTUPINFO
	{
		public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
		public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
		public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
	}
	[StructLayout(LayoutKind.Sequential)]
	struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
	[StructLayout(LayoutKind.Sequential)]
	public struct RECT { public int Left, Top, Right, Bottom; }
	delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);

	[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
	static extern bool CreateProcessW(string app, StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string dir, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
	[DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
	[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
	[DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
	[DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
	[DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
	[DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
	[DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
	[DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
	[DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr lParam);
	[DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
	[DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
	[DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
	[DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr value);
	[DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);

	const int SW_MINIMIZE = 6, SW_SHOWNOACTIVATE = 4;
	const uint SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOACTIVATE = 0x10;
	static readonly IntPtr HWND_BOTTOM = new IntPtr(1);

	public static void MakeDpiAware()
	{
		SetProcessDpiAwarenessContext(new IntPtr(-4)); // per-monitor aware v2: physical pixels
	}

	// Starts `exe args` without activating it; returns the process id
	public static int Start(string exe, string args)
	{
		STARTUPINFO si = new STARTUPINFO();
		si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
		si.dwFlags = 1; // STARTF_USESHOWWINDOW
		si.wShowWindow = SW_SHOWNOACTIVATE;
		PROCESS_INFORMATION pi;
		StringBuilder cmd = new StringBuilder("\"" + exe + "\" " + args);
		if (!CreateProcessW(exe, cmd, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, null, ref si, out pi))
			throw new Exception("CreateProcess failed: " + Marshal.GetLastWin32Error());
		CloseHandle(pi.hThread);
		CloseHandle(pi.hProcess);
		return pi.dwProcessId;
	}

	static uint PidOf(IntPtr h) { uint pid; GetWindowThreadProcessId(h, out pid); return pid; }
	static string Title(IntPtr h) { StringBuilder text = new StringBuilder(512); GetWindowText(h, text, 512); return text.ToString(); }

	public static string Describe(IntPtr h)
	{
		string name = "?";
		try { name = Process.GetProcessById((int)PidOf(h)).ProcessName; } catch (Exception) { }
		return String.Format("{0} {1} \"{2}\"", h, name, Title(h));
	}

	// Studio's main window: a visible top-level window of the process titled "... - Roblox Studio"
	public static IntPtr MainWindow(int pid)
	{
		IntPtr found = IntPtr.Zero;
		EnumWindows((h, _) =>
		{
			if (PidOf(h) == (uint)pid && IsWindowVisible(h) && Title(h).EndsWith("Roblox Studio")) found = h;
			return found == IntPtr.Zero;
		}, IntPtr.Zero);
		return found;
	}

	// Moves the window fully above the virtual screen, in line with the primary monitor (whose display scale it
	// keeps), at the bottom of the z-order, without activating it. Returns its new rectangle.
	public static string Hide(IntPtr h, int width, int height)
	{
		if (IsZoomed(h) || IsIconic(h)) ShowWindow(h, SW_SHOWNOACTIVATE);
		int virtualTop = GetSystemMetrics(77); // SM_YVIRTUALSCREEN
		int y = virtualTop - height - 200;
		SetWindowPos(h, HWND_BOTTOM, 0, y, width, height, SWP_NOACTIVATE);
		RECT r;
		GetWindowRect(h, out r);
		return String.Format("{0},{1} {2}x{3} dpi={4}", r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top, GetDpiForWindow(h));
	}

	// Posts a mouse move to the bottom-right corner of every large child window (the viewport is one): Studio then
	// draws its cursor there instead of at the viewport's centre. No real input; the cursor does not move.
	public static int ParkCursor(IntPtr main)
	{
		int posted = 0;
		EnumChildWindows(main, (child, _) =>
		{
			RECT r;
			GetWindowRect(child, out r);
			int w = r.Right - r.Left, hh = r.Bottom - r.Top;
			if (IsWindowVisible(child) && w >= 400 && hh >= 300)
			{
				int x = w - 2, y = hh - 2;
				PostMessageW(child, 0x0200, IntPtr.Zero, new IntPtr((y << 16) | (x & 0xFFFF))); // WM_MOUSEMOVE
				posted++;
			}
			return true;
		}, IntPtr.Zero);
		return posted;
	}

	// Watches the foreground for `ms` milliseconds (or until the process exits): a window of `pid` that becomes the
	// foreground is put back behind and `previous` gets the foreground again. With `hide`, Studio's main window is
	// moved off the visible desktop as soon as it exists. Returns a log, one line per event.
	public static string Guard(int pid, IntPtr previous, int ms, bool hide, int width, int height)
	{
		StringBuilder log = new StringBuilder();
		Stopwatch clock = Stopwatch.StartNew();
		IntPtr last = GetForegroundWindow();
		bool placed = !hide;
		log.AppendLine("0 ms: foreground " + Describe(last));
		while (clock.ElapsedMilliseconds < ms)
		{
			bool alive = true;
			try { alive = !Process.GetProcessById(pid).HasExited; } catch (Exception) { alive = false; }
			if (!alive) { log.AppendLine(clock.ElapsedMilliseconds + " ms: Studio exited"); break; }
			if (!placed)
			{
				IntPtr main = MainWindow(pid);
				if (main != IntPtr.Zero)
				{
					log.AppendLine(clock.ElapsedMilliseconds + " ms: main window " + Describe(main) + " moved to " + Hide(main, width, height));
					placed = true;
				}
			}
			IntPtr now = GetForegroundWindow();
			if (now != last)
			{
				log.AppendLine(clock.ElapsedMilliseconds + " ms: foreground " + Describe(now));
				last = now;
				if (now != IntPtr.Zero && PidOf(now) == (uint)pid)
				{
					ShowWindow(now, SW_MINIMIZE);
					ShowWindow(now, SW_SHOWNOACTIVATE);
					SetWindowPos(now, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
					if (previous != IntPtr.Zero && IsWindow(previous)) SetForegroundWindow(previous);
					last = GetForegroundWindow();
					log.AppendLine(clock.ElapsedMilliseconds + " ms: STOLEN, put back; foreground " + Describe(last));
				}
			}
			Thread.Sleep(15);
		}
		return log.ToString();
	}
}
