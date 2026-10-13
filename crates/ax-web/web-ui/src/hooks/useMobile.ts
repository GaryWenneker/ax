import {useEffect, useState} from 'react';

export default function useMobile() {
  const [mobile, setMobile] = useState(() => window.matchMedia('(max-width: 899px)').matches);
  useEffect(() => {
    const query = window.matchMedia('(max-width: 899px)');
    const update = () => setMobile(query.matches);
    query.addEventListener('change', update);
    update();
    return () => query.removeEventListener('change', update);
  }, []);
  return mobile;
}
