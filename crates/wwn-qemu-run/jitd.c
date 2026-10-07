/* iOS 26 TXM JIT broker for wwn-qemu-run.
 * Stays attached (P_TRACED) so QEMU's split-wx path hits brk #0x69,
 * then mach_vm_protects the RX region the guest passed in x0/x1.
 */
#include <mach/mach.h>
#include <mach/vm_map.h>
#include <mach/thread_act.h>
#include <mach/arm/thread_status.h>
#include <sys/types.h>
#include <spawn.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <sys/wait.h>

#ifndef PT_ATTACH
#define PT_ATTACH 10
#endif
#ifndef PT_ATTACHEXC
#define PT_ATTACHEXC 14
#endif
#ifndef PT_CONTINUE
#define PT_CONTINUE 7
#endif
#ifndef PT_DETACH
#define PT_DETACH 11
#endif

#define BRK69 0xD4200D20u

int ptrace(int request, pid_t pid, void *addr, int data);
extern char **environ;

static void die(const char *msg) {
  perror(msg);
  exit(1);
}

static int handle_brk(task_t task) {
  thread_act_array_t threads = NULL;
  mach_msg_type_number_t n = 0;
  kern_return_t kr = task_threads(task, &threads, &n);
  if (kr != KERN_SUCCESS || n == 0) {
    fprintf(stderr, "wwn-qemu-jitd: task_threads kr=%d n=%u\n", kr, n);
    return -1;
  }
  arm_thread_state64_t st;
  mach_msg_type_number_t count = ARM_THREAD_STATE64_COUNT;
  kr = thread_get_state(threads[0], ARM_THREAD_STATE64, (thread_state_t)&st,
                        &count);
  if (kr != KERN_SUCCESS) {
    fprintf(stderr, "wwn-qemu-jitd: thread_get_state kr=%d\n", kr);
    return -1;
  }
  uint64_t pc = arm_thread_state64_get_pc(st);
  uint32_t insn = 0;
  vm_size_t got = 0;
  vm_read_overwrite(task, (vm_address_t)pc, 4, (vm_address_t)&insn, &got);
  fprintf(stderr, "wwn-qemu-jitd: stop pc=0x%llx insn=0x%08x x0=0x%llx x1=0x%llx\n",
          (unsigned long long)pc, insn, (unsigned long long)st.__x[0],
          (unsigned long long)st.__x[1]);
  if (insn == BRK69 || (insn & 0xFFE0001F) == 0xD4200000) {
    vm_address_t addr = (vm_address_t)st.__x[0];
    vm_size_t len = (vm_size_t)st.__x[1];
    kr = vm_protect(task, addr, len, FALSE,
                    VM_PROT_READ | VM_PROT_WRITE | VM_PROT_EXECUTE);
    fprintf(stderr, "wwn-qemu-jitd: vm_protect RWX 0x%llx+%lx kr=%d\n",
            (unsigned long long)addr, (unsigned long)len, kr);
    if (kr != KERN_SUCCESS) {
      kr = vm_protect(task, addr, len, FALSE, VM_PROT_READ | VM_PROT_EXECUTE);
      fprintf(stderr, "wwn-qemu-jitd: vm_protect RX kr=%d\n", kr);
    }
    arm_thread_state64_set_pc_fptr(st, (void *)(uintptr_t)(pc + 4));
    kr = thread_set_state(threads[0], ARM_THREAD_STATE64, (thread_state_t)&st,
                          ARM_THREAD_STATE64_COUNT);
    fprintf(stderr, "wwn-qemu-jitd: skip brk set_state kr=%d\n", kr);
    return 1;
  }
  return 0;
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: wwn-qemu-jitd <qemu-run> [args...]\n");
    return 2;
  }
  posix_spawnattr_t attr;
  posix_spawnattr_init(&attr);
  posix_spawnattr_setflags(&attr, POSIX_SPAWN_START_SUSPENDED);
  pid_t child = 0;
  int rc = posix_spawn(&child, argv[1], NULL, &attr, argv + 1, environ);
  posix_spawnattr_destroy(&attr);
  if (rc != 0) {
    errno = rc;
    die("posix_spawn");
  }
  fprintf(stderr, "wwn-qemu-jitd: spawned pid=%d\n", (int)child);
  if (ptrace(PT_ATTACH, child, 0, 0) != 0) {
    perror("wwn-qemu-jitd: PT_ATTACH");
    if (ptrace(PT_ATTACHEXC, child, 0, 0) != 0) {
      perror("wwn-qemu-jitd: PT_ATTACHEXC");
    }
  }
  task_t task = MACH_PORT_NULL;
  kern_return_t kr = task_for_pid(mach_task_self(), child, &task);
  fprintf(stderr, "wwn-qemu-jitd: task_for_pid kr=%d\n", kr);
  kill(child, SIGCONT);
  for (;;) {
    int status = 0;
    pid_t w = waitpid(child, &status, WUNTRACED);
    if (w < 0) {
      if (errno == EINTR) {
        continue;
      }
      die("waitpid");
    }
    if (WIFEXITED(status)) {
      fprintf(stderr, "wwn-qemu-jitd: child exit %d\n", WEXITSTATUS(status));
      return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
      fprintf(stderr, "wwn-qemu-jitd: child signal %d\n", WTERMSIG(status));
      return 128 + WTERMSIG(status);
    }
    if (WIFSTOPPED(status)) {
      int sig = WSTOPSIG(status);
      fprintf(stderr, "wwn-qemu-jitd: stopped sig=%d\n", sig);
      if (task != MACH_PORT_NULL) {
        handle_brk(task);
      }
      if (ptrace(PT_CONTINUE, child, (void *)1, 0) != 0) {
        perror("wwn-qemu-jitd: PT_CONTINUE");
      }
    }
  }
}
