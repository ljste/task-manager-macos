#include "SystemBridge.h"
#include <libproc.h>
#include <errno.h>
#include <sys/proc_info.h>
#include <sys/proc.h>
#include <sys/resource.h>
#include <sys/sysctl.h>
#include <sys/mount.h>
#include <sys/socket.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <mach/machine.h>
#include <pwd.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <time.h>
#include <ifaddrs.h>
#include <net/if.h>
#include <net/route.h>
#include <net/if_dl.h>
#include <stddef.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <utmpx.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>
static uint64_t sysnum(const char *key) { uint64_t v=0; size_t n=sizeof(v); sysctlbyname(key,&v,&n,NULL,0); return v; }
uint64_t tm_ticks_to_ns(uint64_t ticks) { mach_timebase_info_data_t timebase; mach_timebase_info(&timebase); return (uint64_t)((__uint128_t)ticks * timebase.numer / timebase.denom); }
void tm_free(void *p) { free(p); }
void tm_username(int uid, char *name, int capacity) {
 char buffer[16384];
 struct passwd account, *result = NULL;
 if (getpwuid_r(uid, &account, buffer, sizeof(buffer), &result) == 0 && result)
  strlcpy(name, account.pw_name, capacity);
 else
  snprintf(name, capacity, "%d", uid);
}

int tm_process_image(int pid, char *path, int capacity) {
 path[0] = 0;
 proc_pidpath(pid, path, capacity);
 struct proc_archinfo arch;
 return proc_pidinfo(pid, PROC_PIDARCHINFO, 0, &arch, sizeof(arch)) == sizeof(arch) ? arch.p_cputype : 0;
}

static void listed_identity(const struct kinfo_proc *listed, TMProcess *p) {
 p->pid = listed->kp_proc.p_pid;
 p->ppid = listed->kp_eproc.e_ppid;
 p->uid = listed->kp_eproc.e_ucred.cr_uid;
 p->state = listed->kp_proc.p_stat;
 p->nice_value = listed->kp_proc.p_nice;
 p->start_sec = listed->kp_proc.p_starttime.tv_sec;
 p->start_usec = listed->kp_proc.p_starttime.tv_usec;
 strncpy(p->name, listed->kp_proc.p_comm, sizeof(listed->kp_proc.p_comm));
 p->name[sizeof(listed->kp_proc.p_comm)] = 0;
}

static void bsd_name(const struct proc_bsdinfo *bsd, char *name, size_t capacity) {
 const char *source = bsd->pbi_name[0] ? bsd->pbi_name : bsd->pbi_comm;
 size_t length = bsd->pbi_name[0] ? sizeof(bsd->pbi_name) : sizeof(bsd->pbi_comm);
 size_t count = strnlen(source, length);
 if (count >= capacity) count = capacity - 1;
 memcpy(name, source, count);
 name[count] = 0;
}

int tm_sample_image(const TMProcess *p, char *path, int capacity, char name[256], int *arch) {
 struct proc_bsdinfo bsd;
 if (proc_pidinfo(p->pid, PROC_PIDTBSDINFO, 0, &bsd, sizeof(bsd)) == sizeof(bsd)) {
  if (bsd.pbi_start_tvsec != p->start_sec || bsd.pbi_start_tvusec != p->start_usec ||
      (int32_t)bsd.pbi_uid != p->uid) return 0;
  bsd_name(&bsd, name, sizeof(p->name));
 } else if (name != p->name) {
  strlcpy(name, p->name, sizeof(p->name));
 }
 *arch = tm_process_image(p->pid, path, capacity);
 return 1;
}

static void read_task_stats(TMProcess *p, const struct proc_taskinfo *task) {
 p->metrics_valid = 1;
 p->cpu_ns = tm_ticks_to_ns(task->pti_total_user + task->pti_total_system);
 p->memory = task->pti_resident_size;
 p->threads = task->pti_threadnum;
}

static int read_usage(TMProcess *p, struct rusage_info_v2 *usage) {
 if (proc_pid_rusage(p->pid, RUSAGE_INFO_V2, (rusage_info_t *)usage)) return 0;
 p->metrics_valid = 1;
 p->io_valid = 1;
 p->cpu_ns = tm_ticks_to_ns(usage->ri_user_time + usage->ri_system_time);
 p->memory = usage->ri_resident_size;
 p->read_bytes = usage->ri_diskio_bytesread;
 p->write_bytes = usage->ri_diskio_byteswritten;
 return 1;
}

int tm_read_process(int pid, TMProcess *p) {
 memset(p, 0, sizeof(*p));
 p->pid = pid;
 p->threads = -1;
 struct proc_taskallinfo all;
 if (proc_pidinfo(pid, PROC_PIDTASKALLINFO, 0, &all, sizeof(all)) == sizeof(all)) {
  const struct proc_bsdinfo *b = &all.pbsd;
  p->ppid = b->pbi_ppid;
  p->uid = b->pbi_uid;
  p->state = b->pbi_status;
  p->nice_value = b->pbi_nice;
  p->start_sec = b->pbi_start_tvsec;
  p->start_usec = b->pbi_start_tvusec;
  bsd_name(b, p->name, sizeof(p->name));
  read_task_stats(p, &all.ptinfo);
  p->arch = tm_process_image(pid, p->path, sizeof(p->path));
 } else {
  int mib[] = {CTL_KERN, KERN_PROC, KERN_PROC_PID, pid};
  struct kinfo_proc current;
  size_t size = sizeof(current);
  if (sysctl(mib, 4, &current, &size, NULL, 0) || size != sizeof(current)) return 0;
  listed_identity(&current, p);
  if (!tm_sample_image(p, p->path, sizeof(p->path), p->name, &p->arch)) return 0;
 }
 struct rusage_info_v2 usage;
 read_usage(p, &usage);
 tm_username(p->uid, p->user, sizeof(p->user));
 return 1;
}

void tm_visit_processes(int with_threads, void *context, TMProcessVisitor visit) {
 const uint8_t no_uuid[16] = {0};
 int mib[] = {CTL_KERN, KERN_PROC, KERN_PROC_ALL, 0};
 for (int attempt = 0; attempt < 3; attempt++) {
  size_t size = 0;
  if (sysctl(mib, 4, NULL, &size, NULL, 0)) break;
  size += 32 * sizeof(struct kinfo_proc);
  struct kinfo_proc *list = malloc(size);
  if (!list) return;
  if (sysctl(mib, 4, list, &size, NULL, 0)) {
   int error = errno;
   free(list);
   if (error == ENOMEM) continue;
   break;
  }
  TMProcess process = {0};
  for (size_t i = 0; i < size / sizeof(*list); i++) {
   if (list[i].kp_proc.p_pid <= 0) continue;
   memset(&process, 0, offsetof(TMProcess, name));
   listed_identity(&list[i], &process);
   process.threads = -1;
   struct rusage_info_v2 usage = {0};
   int have_usage = read_usage(&process, &usage);
   if (with_threads || !have_usage) {
    struct proc_taskinfo task;
    if (proc_pidinfo(process.pid, PROC_PIDTASKINFO, 0, &task, sizeof(task)) == sizeof(task)) {
     if (have_usage) process.threads = task.pti_threadnum;
     else read_task_stats(&process, &task);
    }
   }
   visit(context, &process, have_usage ? usage.ri_uuid : no_uuid,
         have_usage ? usage.ri_proc_start_abstime : 0);
  }
  free(list);
  return;
 }
 TMProcess *items = NULL;
 int count = tm_processes(&items);
 for (int i = 0; i < count; i++) visit(context, &items[i], no_uuid, 0);
 free(items);
}

int tm_processes(TMProcess **out) {
 *out = NULL;
 int bytes=proc_listpids(PROC_ALL_PIDS,0,NULL,0); int capacity=bytes+4096;
 int *pids=calloc(1,capacity); if(!pids) return 0;
 int n=proc_listpids(PROC_ALL_PIDS,0,pids,capacity)/(int)sizeof(int);
 TMProcess *items=calloc(n?n:1,sizeof(TMProcess)); if (!items) { free(pids); return 0; } int count=0;
 for(int i=0;i<n;i++) if(pids[i]>0 && tm_read_process(pids[i],&items[count])) count++;
 free(pids); *out=items; return count;
}
static uint64_t number(CFDictionaryRef d, const char *key) {
 CFStringRef k=CFStringCreateWithCString(NULL,key,kCFStringEncodingUTF8); CFTypeRef v=CFDictionaryGetValue(d,k); int64_t n=0;
 if(v && CFGetTypeID(v)==CFNumberGetTypeID()) CFNumberGetValue(v,kCFNumberSInt64Type,&n); CFRelease(k); return n>0?n:0;
}
void tm_system(TMSystem *s) {
 memset(s,0,sizeof(*s)); host_cpu_load_info_data_t cpu; mach_msg_type_number_t nc=HOST_CPU_LOAD_INFO_COUNT;
 mach_port_t host=mach_host_self();
 if(host_statistics(host,HOST_CPU_LOAD_INFO,(host_info_t)&cpu,&nc)==KERN_SUCCESS) {
 s->cpu_user=cpu.cpu_ticks[CPU_STATE_USER]; s->cpu_system=cpu.cpu_ticks[CPU_STATE_SYSTEM]; s->cpu_idle=cpu.cpu_ticks[CPU_STATE_IDLE]; s->cpu_nice=cpu.cpu_ticks[CPU_STATE_NICE]; }
 vm_statistics64_data_t vm; nc=HOST_VM_INFO64_COUNT; vm_size_t page; host_page_size(host,&page);
 if(host_statistics64(host,HOST_VM_INFO64,(host_info64_t)&vm,&nc)==KERN_SUCCESS) {
 s->active=(uint64_t)vm.active_count*page; s->inactive=(uint64_t)vm.inactive_count*page; s->wired=(uint64_t)vm.wire_count*page;
 s->compressed=(uint64_t)vm.compressor_page_count*page; s->free_bytes=(uint64_t)vm.free_count*page;
 s->purgeable=(uint64_t)vm.purgeable_count*page; s->speculative=(uint64_t)vm.speculative_count*page; }
 mach_port_deallocate(mach_task_self(),host);
 s->ram=sysnum("hw.memsize"); s->logical=(int)sysnum("hw.logicalcpu"); s->physical=(int)sysnum("hw.physicalcpu");
 s->performance=(int)sysnum("hw.perflevel0.logicalcpu"); s->efficiency=(int)sysnum("hw.perflevel1.logicalcpu");
 s->pressure=(int)sysnum("kern.memorystatus_vm_pressure_level");
 size_t sz=sizeof(s->cpu_brand); sysctlbyname("machdep.cpu.brand_string",s->cpu_brand,&sz,NULL,0);
 struct xsw_usage swap; sz=sizeof(swap); if(sysctlbyname("vm.swapusage",&swap,&sz,NULL,0)==0){s->swap_used=swap.xsu_used;s->swap_total=swap.xsu_total;}
 struct timeval boot; sz=sizeof(boot); if(sysctlbyname("kern.boottime",&boot,&sz,NULL,0)==0) s->uptime=difftime(time(NULL),boot.tv_sec);
 double load[3]; if(getloadavg(load,3)==3){s->load1=load[0];s->load5=load[1];s->load15=load[2];}
 struct statfs fs={0}; if(statfs("/System/Volumes/Data",&fs)!=0) statfs("/",&fs);
 s->disk_capacity=(uint64_t)fs.f_blocks*fs.f_bsize; s->disk_free=(uint64_t)fs.f_bavail*fs.f_bsize;
 snprintf(s->filesystem,sizeof(s->filesystem),"%s",fs.f_fstypename); snprintf(s->mount,sizeof(s->mount),"%s",fs.f_mntonname); snprintf(s->device,sizeof(s->device),"%s",fs.f_mntfromname);
 io_iterator_t it;
 if(IOServiceGetMatchingServices(kIOMainPortDefault,IOServiceMatching("IOBlockStorageDriver"),&it)==KERN_SUCCESS) {
 io_object_t obj; while((obj=IOIteratorNext(it))) {
 CFTypeRef stats=IORegistryEntryCreateCFProperty(obj,CFSTR("Statistics"),NULL,0);
 if(stats && CFGetTypeID(stats)==CFDictionaryGetTypeID()){s->disk_read+=number(stats,"Bytes (Read)");s->disk_write+=number(stats,"Bytes (Write)");s->disk_valid=1;}
 if(stats) CFRelease(stats); IOObjectRelease(obj); } IOObjectRelease(it); }
 int mib[6]={CTL_NET,PF_ROUTE,0,0,NET_RT_IFLIST2,0}; sz=0;
 if(sysctl(mib,6,NULL,&sz,NULL,0)==0){ char *buf=malloc(sz);
 if (buf && sysctl(mib, 6, buf, &sz, NULL, 0) == 0) {
  for (size_t offset = 0; sz - offset >= sizeof(struct if_msghdr);) {
   const struct if_msghdr *msg = (const void *)(buf + offset);
   size_t length = msg->ifm_msglen;
   if (!length || length > sz - offset) break;
   if (msg->ifm_type == RTM_IFINFO2 && (msg->ifm_flags & IFF_UP) &&
       length >= sizeof(struct if_msghdr2) + offsetof(struct sockaddr_dl, sdl_data) + 2) {
    const struct if_msghdr2 *m = (const void *)msg;
    const struct sockaddr_dl *link = (const void *)(m + 1);
    if (m->ifm_addrs == RTA_IFP && link->sdl_family == AF_LINK && link->sdl_nlen >= 2 &&
        memcmp(link->sdl_data, "en", 2) == 0) {
     s->net_in += m->ifm_data.ifi_ibytes;
     s->net_out += m->ifm_data.ifi_obytes;
    }
   }
   offset += length;
  }
 }
 free(buf); }
 struct ifaddrs *ifa=NULL; if(getifaddrs(&ifa)==0){for(struct ifaddrs *i=ifa;i;i=i->ifa_next){if(!i->ifa_addr || !(i->ifa_flags&IFF_UP) || strncmp(i->ifa_name,"en",2)!=0)continue;
 if(i->ifa_addr->sa_family==AF_INET){char ip[INET_ADDRSTRLEN],entry[128];inet_ntop(AF_INET,&((struct sockaddr_in*)i->ifa_addr)->sin_addr,ip,sizeof(ip));snprintf(entry,sizeof(entry),"%s  %s\n",i->ifa_name,ip);strlcat(s->interfaces,entry,sizeof(s->interfaces));}}
 freeifaddrs(ifa);}
}
int tm_arguments(int pid,char *buffer,int capacity){
 int mib[3]={CTL_KERN,KERN_PROCARGS2,pid}; size_t n=0;
 if(sysctl(mib,3,NULL,&n,NULL,0)!=0 || n<sizeof(int) || n>1024*1024) return 0;
 char *raw=calloc(1,n); if(!raw)return 0;if(sysctl(mib,3,raw,&n,NULL,0)!=0){free(raw);return 0;}
 int argc=0;memcpy(&argc,raw,sizeof(int)); char *p=raw+sizeof(int),*end=raw+n;
 while(p<end && *p)p++;while(p<end && !*p)p++;
 buffer[0]=0;for(int i=0;i<argc && p<end;i++){size_t len=strnlen(p,end-p);if(p+len>=end)break;if(i)strlcat(buffer," ",capacity);strlcat(buffer,p,capacity);p+=len+1;}
 free(raw);return 1;
}
int tm_open_files(int pid){int n=proc_pidinfo(pid,PROC_PIDLISTFDS,0,NULL,0);return n>0?n/(int)sizeof(struct proc_fdinfo):-1;}
int tm_sessions(char *buffer,int capacity){buffer[0]=0;setutxent();struct utmpx *u;int n=0;while((u=getutxent()))if(u->ut_type==USER_PROCESS){char line[512];snprintf(line,sizeof(line),"%.*s\t%.*s\n",(int)sizeof(u->ut_user),u->ut_user,(int)sizeof(u->ut_line),u->ut_line);strlcat(buffer,line,capacity);n++;}endutxent();return n;}

int tm_cpu_load(TMCPU **out) {
 *out = NULL;
 processor_info_array_t info = NULL;
 mach_msg_type_number_t count = 0;
 natural_t processors = 0;
 mach_port_t host = mach_host_self();
 kern_return_t status = host_processor_info(host, PROCESSOR_CPU_LOAD_INFO, &processors, &info, &count);
 mach_port_deallocate(mach_task_self(), host);
 if (status != KERN_SUCCESS) return 0;
 TMCPU *result = calloc(processors, sizeof(TMCPU));
 if (result && count >= processors * CPU_STATE_MAX) {
  for (natural_t i = 0; i < processors; i++) {
   result[i].user = (uint32_t)info[i * CPU_STATE_MAX + CPU_STATE_USER];
   result[i].system = (uint32_t)info[i * CPU_STATE_MAX + CPU_STATE_SYSTEM];
   result[i].idle = (uint32_t)info[i * CPU_STATE_MAX + CPU_STATE_IDLE];
   result[i].nice = (uint32_t)info[i * CPU_STATE_MAX + CPU_STATE_NICE];
  }
 } else { free(result); result = NULL; }
 vm_deallocate(mach_task_self(), (vm_address_t)info, count * sizeof(integer_t));
 *out = result;
 return result ? (int)processors : 0;
}
