#ifndef __GBLIBC_POLL_H_
#define __GBLIBC_POLL_H_

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned int nfds_t;

#define POLLIN          0x0001          /* any readable data available */
#define POLLPRI         0x0002          /* OOB/Urgent readable data */
#define POLLOUT         0x0004          /* file descriptor is writeable */
#define POLLRDNORM      0x0040          /* non-OOB/URG data available */
#define POLLWRNORM      POLLOUT         /* no write type differentiation */
#define POLLRDBAND      0x0080          /* OOB/Urgent readable data */
#define POLLWRBAND      0x0100          /* OOB/Urgent data can be written */

#define POLLERR         0x0008          /* some poll error occurred */
#define POLLHUP         0x0010          /* file descriptor was "hung up" */
#define POLLNVAL        0x0020          /* requested events "invalid" */

#define POLLSTANDARD    (POLLIN|POLLPRI|POLLOUT|POLLRDNORM|POLLRDBAND|\
	                 POLLWRBAND|POLLERR|POLLHUP|POLLNVAL)

struct pollfd {
	int     fd;
	short   events;
	short   revents;
};

#ifdef __cplusplus
}
#endif

#endif
