/**
 * WebRTC leak detection helper.
 * Uses RTCPeerConnection with parallel STUN servers to discover gathered ICE candidate IPs.
 * Resolves within ~100-200ms once candidates are gathered (capped at 1200ms timeout).
 */

const PRIVATE_IP_REGEX = /^(127\.|10\.|172\.(1[6-9]|2[0-9]|3[0-1])\.|192\.168\.|169\.254\.|100\.(6[4-9]|[7-9][0-9]|1[0-1][0-9]|12[0-7])\.|fc[0-9a-f]{2}:|fd[0-9a-f]{2}:|fe[89ab][0-9a-f]:|::1)/i;

export async function checkWebRtcLeak(expectedPublicIp = null, timeoutMs = 1200) {
  if (typeof window === 'undefined' || !window.RTCPeerConnection) {
    return {
      isLeaking: false,
      candidateIps: [],
      statusText: 'WebRTC API недоступен (защищено)',
    };
  }

  return new Promise((resolve) => {
    const candidateIps = new Set();
    let pc = null;
    let timer = null;
    let earlyTimer = null;

    const cleanup = () => {
      if (timer) clearTimeout(timer);
      if (earlyTimer) clearTimeout(earlyTimer);
      if (pc) {
        try {
          pc.close();
        } catch (_) {}
        pc = null;
      }
    };

    const finish = () => {
      cleanup();
      const ipList = Array.from(candidateIps);
      const publicCandidates = ipList.filter((ip) => !PRIVATE_IP_REGEX.test(ip));

      let isLeaking = false;
      let statusText = 'WebRTC защищён: утечек не обнаружено';

      if (publicCandidates.length > 0 && expectedPublicIp) {
        const expectedTrimmed = expectedPublicIp.trim();
        const expectedIsIpv6 = expectedTrimmed.includes(':');
        const leaked = publicCandidates.filter((ip) => {
          const candIsIpv6 = ip.includes(':');
          if (candIsIpv6 !== expectedIsIpv6) {
            return false;
          }
          return ip.toLowerCase() !== expectedTrimmed.toLowerCase();
        });
        if (leaked.length > 0) {
          isLeaking = true;
          statusText = `Обнаружена утечка через WebRTC: ${leaked.join(', ')}`;
        }
      }

      resolve({
        isLeaking,
        candidateIps: ipList,
        publicCandidates,
        statusText,
      });
    };

    timer = setTimeout(finish, timeoutMs);

    try {
      pc = new RTCPeerConnection({
        iceServers: [
          { urls: ['stun:stun.l.google.com:19302', 'stun:stun.cloudflare.com:3478'] },
        ],
      });

      pc.onicecandidate = (event) => {
        if (!event || !event.candidate || !event.candidate.candidate) {
          return;
        }
        const cand = event.candidate.candidate;
        const parts = cand.split(' ');
        if (parts.length >= 5) {
          const addr = parts[4];
          if (/^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$/.test(addr) || /^[a-f0-9:]+$/i.test(addr)) {
            candidateIps.add(addr);
          }
        }
        const matchIpv4 = cand.match(/([0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3})/);
        if (matchIpv4 && matchIpv4[1]) {
          candidateIps.add(matchIpv4[1]);
        }
        const matchIpv6 = cand.match(/([a-f0-9:]{3,39})/i);
        if (matchIpv6 && matchIpv6[1] && matchIpv6[1].includes(':')) {
          candidateIps.add(matchIpv6[1]);
        }

        // Fast resolution once candidates arrive
        if (candidateIps.size > 0 && !earlyTimer) {
          earlyTimer = setTimeout(finish, 60);
        }
      };

      pc.onicegatheringstatechange = () => {
        if (pc && pc.iceGatheringState === 'complete') {
          finish();
        }
      };

      pc.createDataChannel('leak-check');
      pc.createOffer()
        .then((offer) => {
          if (pc) return pc.setLocalDescription(offer);
        })
        .catch(() => finish());
    } catch (_) {
      finish();
    }
  });
}
