using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class MediaBrowserTokenProbe {
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,int pid);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr h);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr t,int c,IntPtr b,int n,out int needed);
 [DllImport("advapi32.dll")] [return:MarshalAs(UnmanagedType.Bool)] static extern bool IsTokenRestricted(IntPtr t);
 [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertSidToStringSid(IntPtr sid,out IntPtr str);
 static IntPtr Info(IntPtr token,int kind) {int n;GetTokenInformation(token,kind,IntPtr.Zero,0,out n);if(n<=0)throw new Win32Exception(Marshal.GetLastWin32Error());IntPtr p=Marshal.AllocHGlobal(n);if(!GetTokenInformation(token,kind,p,n,out n)){Marshal.FreeHGlobal(p);throw new Win32Exception(Marshal.GetLastWin32Error());}return p;}
 static string Sid(IntPtr p){IntPtr text;if(!ConvertSidToStringSid(p,out text))throw new Win32Exception(Marshal.GetLastWin32Error());try{return Marshal.PtrToStringUni(text);}finally{LocalFree(text);}}
 public static object Read(){return ReadHandle(GetCurrentProcess());}
 public static object ReadPid(int pid){IntPtr process=OpenProcess(0x1000,false,pid);if(process==IntPtr.Zero)throw new Win32Exception(Marshal.GetLastWin32Error());try{return ReadHandle(process);}finally{CloseHandle(process);}}
 static object ReadHandle(IntPtr process){IntPtr token;if(!OpenProcessToken(process,8,out token))throw new Win32Exception(Marshal.GetLastWin32Error());try{
 IntPtr g=Info(token,11),il=Info(token,25);try{
 int count=Marshal.ReadInt32(g);var groups=new List<string>();int offset=IntPtr.Size==8?8:4;int stride=IntPtr.Size==8?16:8;for(int i=0;i<count;i++)groups.Add(Sid(Marshal.ReadIntPtr(g,offset+i*stride)));
 string label=Sid(Marshal.ReadIntPtr(il));return new {restricted=IsTokenRestricted(token),restricted_sid_count=count,restricted_sids=groups.ToArray(),integrity_sid=label,scope="Read-only token information for the selected diagnostic process; no ACL or token changes"};
 }finally{Marshal.FreeHGlobal(g);Marshal.FreeHGlobal(il);}
 }finally{CloseHandle(token);}}
}
