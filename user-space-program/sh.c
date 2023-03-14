#include <sys/wait.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define print(str) write(0, str, strlen(str))

// struct cmd {
//     enum {
//         pipe,
//         list,
//     } op;
//     struct cmd* left;
//     struct cmd* right;
//     struct token* tk;
// };

// struct token {
//     char* start;
//     size_t len;

//     struct token* next;
// };

// static inline int betwn(char c, char s, char e)
// {
//     return c >= s && c <= e;
// }
// static inline int is_num(char c)
// {
//     return betwn(c, '0', '9');
// }
// static inline int check_token(char c)
// {
//     return betwn(c, 'a', 'z') || betwn(c, 'A', 'Z') || (c == '_');
// }
// static inline int read_token(char* buf, struct token** cur)
// {
//     char* p = buf;
//     for (; *p && (check_token(*p) || (p != buf && is_num(*p))); ++p) {
//         (*cur)->start = buf;
//         ++(*cur)->len;
//     }
//     if ((*cur)->start) {
//         (*cur)->next = *cur + 1;
//         *cur = (*cur)->next;
//     }
//     return p - buf;
// }

int main(int argc, const char** argv)
{
    (void)argc, (void)argv;
    char buf[512] = {};

    print("sh # ");

    for (;;) {
        int n = read(0, buf, sizeof(buf));

        char token[1024] = {};
        char* args[128] = { token, 0 };
        int j = 0;
        int k = 1;

        for (int i = 0; i < n; ++i) {
            if (buf[i] == ' ') {
                token[j++] = 0;
                args[k++] = token + j;
                continue;
            }
            if (buf[i] == '\n') {
                token[j++] = 0;

                if (strcmp(args[0], "exit") == 0)
                    return 0;

                pid_t pid = fork();
                if (pid == 0) {
                    char* envp[] = { NULL };
                    int ret = execve(args[0], args, envp);
                    char _b[128];
                    snprintf(_b, sizeof(_b), "sh: execve() failed with code %d\n", ret);
                    print(_b);
                    return -1;
                }

                int code;
                wait(&code);

                char _b[128];
                snprintf(_b, sizeof(_b), "sh (%d) # ", code);
                print(_b);

                j = 0;
                k = 1;
                memset(args, 0x00, sizeof(args));
                args[0] = token;
                continue;
            }
            token[j++] = buf[i];
        }
    }

    return 0;
}
